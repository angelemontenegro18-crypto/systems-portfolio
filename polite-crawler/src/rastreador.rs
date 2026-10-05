//! El rastreador: decide si una URL se puede pedir **ahora**, y si sí, la pide.
//!
//! Nunca duerme. Cuando todavía no toca, devuelve [`Rechazo::Esperar`] con el
//! momento a partir del cual se puede volver a intentar; quien lo usa decide
//! cómo esperar. Eso lo hace determinista con un reloj inyectado, y fácil de
//! integrar en cualquier bucle o planificador.
//!
//! El orden de las comprobaciones, para cada URL:
//!
//! 1. `robots.txt` del origen, leído y vigente. **Leerlo cuenta como una
//!    petición** al host: la primera página llega en la llamada siguiente.
//! 2. La ruta, permitida por `robots.txt`.
//! 3. El presupuesto de páginas del host, sin agotar.
//! 4. El intervalo desde la última petición al host — el mayor entre el
//!    configurado y el `Crawl-delay` — cumplido.
//! 5. Recién entonces, un `GET`.
//!
//! Ante 429, 5xx o una falla de red, el host entra en espera: `Retry-After`
//! si el servidor lo manda (en segundos), si no, un backoff exponencial.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use url::Url;

use crate::identidad::Identidad;
use crate::robots::{Politica, Robots};
use crate::transporte::{Respuesta, Transporte};

/// Límites de cortesía.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Mínimo entre dos peticiones al mismo origen. `Crawl-delay` puede alargarlo, nunca acortarlo.
    pub intervalo_minimo: Duration,
    /// Máximo de páginas por origen.
    pub max_paginas_por_origen: u32,
    /// Primera espera tras un 429, un 5xx o una falla de red; se duplica en cada falla seguida.
    pub espera_inicial: Duration,
    /// Tope del backoff exponencial (no de un `Retry-After` explícito).
    pub espera_maxima: Duration,
    /// Cuánto vale un `robots.txt` leído (la RFC pide no pasar de 24 h).
    pub vigencia_robots: Duration,
    /// Cuánto dura el "todo prohibido" por un `robots.txt` inalcanzable antes de reintentar.
    pub reintento_robots: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            intervalo_minimo: Duration::from_secs(1),
            max_paginas_por_origen: 100,
            espera_inicial: Duration::from_secs(2),
            espera_maxima: Duration::from_secs(600),
            vigencia_robots: Duration::from_secs(24 * 3600),
            reintento_robots: Duration::from_secs(300),
        }
    }
}

/// Por qué todavía no se puede pedir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoEspera {
    /// El intervalo mínimo configurado desde la última petición al origen.
    Intervalo,
    /// El `Crawl-delay` que pide el sitio.
    CrawlDelay,
    /// El servidor pidió esperar (`Retry-After`) o se aplica backoff tras una falla.
    Backoff,
}

/// Por qué no se devolvió una página.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rechazo {
    /// No es una URL válida.
    UrlInvalida(String),
    /// Solo se rastrea `http` y `https`.
    EsquemaNoSoportado(String),
    /// `robots.txt` no permite la ruta. Lleva la regla que decidió, o el motivo.
    ProhibidoPorRobots {
        /// La regla (`Disallow: /privado`) o por qué se asumió todo prohibido.
        motivo: String,
    },
    /// Todavía no toca. Volver a intentar desde `desde_ms`.
    Esperar {
        /// Momento, en el reloj del llamador, a partir del cual se puede pedir.
        desde_ms: u64,
        /// Por qué.
        motivo: MotivoEspera,
    },
    /// Se alcanzó el máximo de páginas para este origen.
    PresupuestoAgotado {
        /// Páginas ya pedidas.
        paginas: u32,
    },
    /// La petición no llegó a tener respuesta. El origen entra en backoff.
    FallaDeRed {
        /// Qué falló.
        mensaje: String,
        /// Desde cuándo se puede reintentar.
        desde_ms: u64,
    },
}

impl fmt::Display for Rechazo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UrlInvalida(u) => write!(f, "URL inválida: {u}"),
            Self::EsquemaNoSoportado(e) => write!(f, "esquema no soportado: {e}"),
            Self::ProhibidoPorRobots { motivo } => write!(f, "robots.txt no lo permite ({motivo})"),
            Self::Esperar { desde_ms, motivo } => {
                write!(f, "esperar hasta t={desde_ms} ms ({motivo:?})")
            }
            Self::PresupuestoAgotado { paginas } => {
                write!(f, "presupuesto agotado: {paginas} páginas")
            }
            Self::FallaDeRed { mensaje, desde_ms } => write!(
                f,
                "falla de red ({mensaje}); reintentar desde t={desde_ms} ms"
            ),
        }
    }
}

impl std::error::Error for Rechazo {}

/// Lo que se obtuvo de una URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagina {
    /// La URL pedida.
    pub url: Url,
    /// Estado HTTP (incluye 4xx: una página que no existe también es una respuesta).
    pub estado: u16,
    /// Cuerpo, posiblemente recortado.
    pub cuerpo: Vec<u8>,
    /// `true` si el cuerpo se recortó.
    pub recortado: bool,
    /// Si fue una redirección, a dónde. **No se sigue sola.**
    pub redireccion: Option<Url>,
    /// Si el servidor pidió esperar (429/5xx), desde cuándo se puede volver a pedir.
    pub espera_hasta_ms: Option<u64>,
}

#[derive(Default)]
struct Origen {
    proxima_ms: u64,
    motivo_proxima: Option<MotivoEspera>,
    paginas: u32,
    fallas_seguidas: u32,
    robots: Option<(Politica, u64, Option<String>)>,
}

/// Rastreador cortés sobre un transporte.
pub struct Rastreador<T: Transporte> {
    transporte: T,
    identidad: Identidad,
    config: Config,
    origenes: HashMap<String, Origen>,
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// `Retry-After` en segundos. La forma de fecha HTTP no se interpreta: se
/// aplica el backoff exponencial en su lugar.
fn retry_after_ms(valor: Option<&str>) -> Option<u64> {
    valor?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|s| s.saturating_mul(1000))
}

impl<T: Transporte> Rastreador<T> {
    /// Un rastreador con esta identidad y estos límites.
    pub fn nuevo(transporte: T, identidad: Identidad, config: Config) -> Self {
        Self {
            transporte,
            identidad,
            config,
            origenes: HashMap::new(),
        }
    }

    /// La identidad con la que se presenta.
    pub fn identidad(&self) -> &Identidad {
        &self.identidad
    }

    /// Intenta obtener `url` en el instante `ahora_ms` del reloj del llamador.
    pub fn obtener(&mut self, url: &str, ahora_ms: u64) -> Result<Pagina, Rechazo> {
        let url = Url::parse(url).map_err(|e| Rechazo::UrlInvalida(format!("{url}: {e}")))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(Rechazo::EsquemaNoSoportado(url.scheme().to_string()));
        }
        let origen = url.origin().ascii_serialization();
        let mut ruta = url.path().to_string();
        if let Some(q) = url.query() {
            ruta.push('?');
            ruta.push_str(q);
        }

        // 1. robots.txt vigente para este origen.
        let vigente = self
            .origenes
            .get(&origen)
            .and_then(|o| o.robots.as_ref())
            .is_some_and(|r| r.1 > ahora_ms);
        if !vigente {
            self.puede_pedir(&origen, ahora_ms)?;
            self.leer_robots(&url, &origen, ahora_ms);
        }

        // 2. La ruta, según robots.txt.
        let o = self.origenes.entry(origen.clone()).or_default();
        if let Some((politica, _, motivo_general)) = &o.robots {
            let d = politica.decidir(&ruta);
            if !d.permitida {
                let motivo = motivo_general
                    .clone()
                    .or(d.regla)
                    .unwrap_or_else(|| "sin regla".into());
                return Err(Rechazo::ProhibidoPorRobots { motivo });
            }
        }

        // 3. Presupuesto.
        if o.paginas >= self.config.max_paginas_por_origen {
            return Err(Rechazo::PresupuestoAgotado { paginas: o.paginas });
        }

        // 4. Intervalo, crawl-delay y backoff.
        self.puede_pedir(&origen, ahora_ms)?;

        // 5. La petición.
        let respuesta = self.transporte.get(&url, self.identidad.user_agent());
        let intervalo = self.intervalo_efectivo(&origen);
        let o = self.origenes.entry(origen).or_default();
        o.paginas += 1;
        o.proxima_ms = ahora_ms.saturating_add(intervalo.0);
        o.motivo_proxima = Some(intervalo.1);

        let r = match respuesta {
            Ok(r) => r,
            Err(mensaje) => {
                let desde_ms = Self::aplicar_backoff(o, &self.config, None, ahora_ms);
                return Err(Rechazo::FallaDeRed { mensaje, desde_ms });
            }
        };

        let espera_hasta_ms = if r.estado == 429 || r.estado >= 500 {
            Some(Self::aplicar_backoff(
                o,
                &self.config,
                r.retry_after.as_deref(),
                ahora_ms,
            ))
        } else {
            o.fallas_seguidas = 0;
            None
        };
        let redireccion = if (300..400).contains(&r.estado) {
            r.location.as_deref().and_then(|l| url.join(l).ok())
        } else {
            None
        };
        Ok(Pagina {
            url,
            estado: r.estado,
            cuerpo: r.cuerpo,
            recortado: r.recortado,
            redireccion,
            espera_hasta_ms,
        })
    }

    /// `Err(Esperar)` si todavía no toca pedirle nada a este origen.
    fn puede_pedir(&self, origen: &str, ahora_ms: u64) -> Result<(), Rechazo> {
        match self.origenes.get(origen) {
            Some(o) if ahora_ms < o.proxima_ms => Err(Rechazo::Esperar {
                desde_ms: o.proxima_ms,
                motivo: o.motivo_proxima.unwrap_or(MotivoEspera::Intervalo),
            }),
            _ => Ok(()),
        }
    }

    /// El intervalo que corresponde al origen, y de dónde sale.
    fn intervalo_efectivo(&self, origen: &str) -> (u64, MotivoEspera) {
        let base = ms(self.config.intervalo_minimo);
        let crawl_delay = self
            .origenes
            .get(origen)
            .and_then(|o| o.robots.as_ref())
            .and_then(|r| r.0.crawl_delay)
            .map(ms)
            .unwrap_or(0);
        if crawl_delay > base {
            (crawl_delay, MotivoEspera::CrawlDelay)
        } else {
            (base, MotivoEspera::Intervalo)
        }
    }

    /// Registra una falla y devuelve desde cuándo se puede volver a pedir.
    fn aplicar_backoff(
        o: &mut Origen,
        config: &Config,
        retry_after: Option<&str>,
        ahora_ms: u64,
    ) -> u64 {
        o.fallas_seguidas = o.fallas_seguidas.saturating_add(1);
        let espera = match retry_after_ms(retry_after) {
            // Lo que pide el servidor se respeta tal cual: el tope es solo para el backoff propio.
            Some(pedido) => pedido,
            None => {
                let factor = 1u64 << (o.fallas_seguidas - 1).min(20);
                ms(config.espera_inicial)
                    .saturating_mul(factor)
                    .min(ms(config.espera_maxima))
            }
        };
        let hasta = ahora_ms.saturating_add(espera).max(o.proxima_ms);
        o.proxima_ms = hasta;
        o.motivo_proxima = Some(MotivoEspera::Backoff);
        hasta
    }

    /// Lee `robots.txt` del origen y aplica la RFC 9309 al resultado:
    /// 2xx se parsea · 4xx (salvo 429) es "no hay reglas" · 429, 5xx o falla de
    /// red es "todo prohibido" hasta reintentar · 3xx se sigue hasta 5 saltos.
    fn leer_robots(&mut self, url: &Url, origen: &str, ahora_ms: u64) {
        let mut destino = url.join("/robots.txt").ok();
        let mut respuesta: Result<Respuesta, String> = Err("sin respuesta".into());
        for _ in 0..=5 {
            let Some(d) = destino.take() else { break };
            respuesta = self.transporte.get(&d, self.identidad.user_agent());
            match &respuesta {
                Ok(r) if (300..400).contains(&r.estado) => {
                    destino = r.location.as_deref().and_then(|l| d.join(l).ok());
                }
                _ => break,
            }
        }

        let vigencia = ms(self.config.vigencia_robots);
        let reintento = ms(self.config.reintento_robots);
        let (robots, dura, motivo) = match &respuesta {
            Ok(r) if (200..300).contains(&r.estado) => (
                Robots::parsear(&String::from_utf8_lossy(&r.cuerpo)),
                vigencia,
                None,
            ),
            Ok(r) if r.estado == 429 || r.estado >= 500 => (
                Robots::prohibir_todo(),
                reintento,
                Some(format!(
                    "robots.txt respondió {}: se asume todo prohibido",
                    r.estado
                )),
            ),
            // 4xx, o una cadena de redirecciones demasiado larga: no hay reglas.
            Ok(_) => (Robots::permitir_todo(), vigencia, None),
            Err(e) => (
                Robots::prohibir_todo(),
                reintento,
                Some(format!(
                    "robots.txt inalcanzable ({e}): se asume todo prohibido"
                )),
            ),
        };

        let politica = robots.politica_para(self.identidad.token());
        let o = self.origenes.entry(origen.to_string()).or_default();
        o.robots = Some((politica, ahora_ms.saturating_add(dura), motivo));
        // Leer robots.txt fue una petición al host: cuenta para el intervalo.
        let (intervalo, motivo) = self.intervalo_efectivo(origen);
        let o = self.origenes.entry(origen.to_string()).or_default();
        o.proxima_ms = o.proxima_ms.max(ahora_ms.saturating_add(intervalo));
        o.motivo_proxima = Some(motivo);
    }
}
