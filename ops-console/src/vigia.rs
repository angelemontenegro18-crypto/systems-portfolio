//! El estado de los servicios del sistema, con **tiempo máximo** y **caché**.
//!
//! - [`VigiaSystemctl`] consulta `systemctl show` (solo en Linux; en otros sistemas responde
//!   [`EstadoServicio::NoDisponible`] con el motivo). Una consulta que no termina en
//!   [`LIMITE`] se corta —el proceso se termina y se espera— y se informa como
//!   [`EstadoServicio::TiempoAgotado`].
//! - [`VigiaConCache`] recuerda cada respuesta durante [`VIDA_CACHE`]: dos peticiones seguidas
//!   no lanzan dos consultas.
//!
//! El tiempo sale de un [`Reloj`] inyectado, así que las pruebas fijan el corte y la caché sin
//! dormir.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::reloj::Reloj;

/// Tiempo máximo de una consulta.
pub const LIMITE: Duration = Duration::from_secs(3);

/// Cuánto vale una respuesta guardada.
pub const VIDA_CACHE: Duration = Duration::from_secs(10);

/// El estado de un servicio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoServicio {
    /// Activo.
    Activo {
        /// Subestado (`running`, `exited`…).
        subestado: String,
        /// Proceso principal, si tiene.
        pid: Option<u32>,
        /// Reinicios automáticos desde que arrancó, si systemd los cuenta.
        reinicios: Option<u32>,
    },
    /// Detenido.
    Inactivo {
        /// Subestado (`dead`…).
        subestado: String,
    },
    /// Falló.
    Fallido {
        /// Subestado.
        subestado: String,
    },
    /// Arrancando, deteniéndose o recargando.
    EnTransicion {
        /// El estado (`activating`…).
        estado: String,
    },
    /// systemd no conoce la unidad.
    NoEncontrado,
    /// No se puede consultar en este sistema.
    NoDisponible {
        /// Por qué.
        motivo: String,
    },
    /// La consulta no terminó a tiempo y se cortó.
    TiempoAgotado {
        /// El límite, en segundos.
        limite_s: u64,
    },
    /// La consulta terminó, pero sin una respuesta que se entienda.
    Error {
        /// Qué pasó.
        motivo: String,
    },
}

impl EstadoServicio {
    /// La clave que usa la API.
    pub fn clave(&self) -> &'static str {
        match self {
            EstadoServicio::Activo { .. } => "activo",
            EstadoServicio::Inactivo { .. } => "inactivo",
            EstadoServicio::Fallido { .. } => "fallido",
            EstadoServicio::EnTransicion { .. } => "en_transicion",
            EstadoServicio::NoEncontrado => "no_encontrado",
            EstadoServicio::NoDisponible { .. } => "no_disponible",
            EstadoServicio::TiempoAgotado { .. } => "tiempo_agotado",
            EstadoServicio::Error { .. } => "error",
        }
    }

    /// Una explicación corta para mostrar.
    pub fn detalle(&self) -> String {
        match self {
            EstadoServicio::Activo {
                subestado,
                pid,
                reinicios,
            } => {
                let mut partes = vec![subestado.clone()];
                if let Some(pid) = pid {
                    partes.push(format!("pid {pid}"));
                }
                if let Some(n) = reinicios {
                    partes.push(format!("{n} reinicios"));
                }
                partes.join(" · ")
            }
            EstadoServicio::Inactivo { subestado } | EstadoServicio::Fallido { subestado } => {
                subestado.clone()
            }
            EstadoServicio::EnTransicion { estado } => estado.clone(),
            EstadoServicio::NoEncontrado => "systemd no conoce esta unidad".into(),
            EstadoServicio::NoDisponible { motivo } | EstadoServicio::Error { motivo } => {
                motivo.clone()
            }
            EstadoServicio::TiempoAgotado { limite_s } => {
                format!("la consulta no respondió en {limite_s} s y se cortó")
            }
        }
    }
}

/// Algo que sabe el estado de un servicio.
pub trait Vigia: Send + Sync {
    /// El estado de `unidad`.
    fn consultar(&self, unidad: &str) -> EstadoServicio;
}

/// Una consulta en marcha, que se puede mirar y cortar.
pub trait Ejecucion {
    /// Su salida, si ya terminó.
    fn terminada(&mut self) -> Option<String>;
    /// La corta: termina el proceso y lo espera.
    fn cortar(&mut self);
}

/// Espera a `ejecucion` hasta `limite`, mirando cada 10 ms del `reloj`. Si no terminó, la
/// corta y devuelve `None`.
pub fn esperar_con_limite(
    ejecucion: &mut dyn Ejecucion,
    limite: Duration,
    reloj: &dyn Reloj,
) -> Option<String> {
    let inicio = reloj.ahora_ms();
    let tope = u64::try_from(limite.as_millis()).unwrap_or(u64::MAX);
    loop {
        if let Some(salida) = ejecucion.terminada() {
            return Some(salida);
        }
        if reloj.ahora_ms().saturating_sub(inicio) >= tope {
            ejecucion.cortar();
            return None;
        }
        reloj.dormir(Duration::from_millis(10));
    }
}

/// Si `unidad` es un nombre de unidad razonable: letras, dígitos y `@._:-`, sin empezar con
/// guion (no se confunde con una opción de `systemctl`), hasta 128 caracteres.
pub fn unidad_valida(unidad: &str) -> bool {
    !unidad.is_empty()
        && unidad.len() <= 128
        && !unidad.starts_with('-')
        && unidad
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._:-".contains(&b))
}

/// Interpreta la salida de `systemctl show --property=LoadState,ActiveState,SubState,…`.
pub fn interpretar(salida: &str) -> EstadoServicio {
    let mut campos: HashMap<&str, &str> = HashMap::new();
    for linea in salida.lines() {
        if let Some((clave, valor)) = linea.split_once('=') {
            campos.insert(clave.trim(), valor.trim());
        }
    }
    if campos.get("LoadState") == Some(&"not-found") {
        return EstadoServicio::NoEncontrado;
    }
    let subestado = campos
        .get("SubState")
        .copied()
        .unwrap_or_default()
        .to_string();
    match campos.get("ActiveState").copied() {
        Some("active") => EstadoServicio::Activo {
            subestado,
            pid: campos
                .get("MainPID")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|&pid| pid > 0),
            reinicios: campos.get("NRestarts").and_then(|v| v.parse().ok()),
        },
        Some("inactive") => EstadoServicio::Inactivo { subestado },
        Some("failed") => EstadoServicio::Fallido { subestado },
        Some(otro @ ("activating" | "deactivating" | "reloading" | "refreshing")) => {
            EstadoServicio::EnTransicion {
                estado: otro.to_string(),
            }
        }
        _ => EstadoServicio::Error {
            motivo: "systemctl respondió algo que no se entiende".into(),
        },
    }
}

/// El vigía real: `systemctl show` con tiempo máximo.
pub struct VigiaSystemctl {
    reloj: Arc<dyn Reloj>,
    limite: Duration,
}

impl VigiaSystemctl {
    /// Un vigía con el límite de [`LIMITE`].
    pub fn nueva(reloj: Arc<dyn Reloj>) -> VigiaSystemctl {
        VigiaSystemctl {
            reloj,
            limite: LIMITE,
        }
    }
}

impl Vigia for VigiaSystemctl {
    fn consultar(&self, unidad: &str) -> EstadoServicio {
        if !unidad_valida(unidad) {
            return EstadoServicio::Error {
                motivo: "nombre de unidad inválido".into(),
            };
        }
        if !cfg!(target_os = "linux") {
            return EstadoServicio::NoDisponible {
                motivo: "systemctl solo existe en Linux".into(),
            };
        }
        let hijo = Command::new("systemctl")
            .args([
                "show",
                unidad,
                "--property=LoadState,ActiveState,SubState,MainPID,NRestarts",
                "--no-pager",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let hijo = match hijo {
            Ok(h) => h,
            Err(e) => {
                return EstadoServicio::NoDisponible {
                    motivo: format!("no se pudo ejecutar systemctl: {e}"),
                }
            }
        };
        let mut ejecucion = EjecucionDeHijo {
            hijo,
            terminado: false,
        };
        match esperar_con_limite(&mut ejecucion, self.limite, self.reloj.as_ref()) {
            Some(salida) => interpretar(&salida),
            None => EstadoServicio::TiempoAgotado {
                limite_s: self.limite.as_secs(),
            },
        }
    }
}

/// Un `systemctl` en marcha. Si se lo suelta sin terminar, se termina igual.
struct EjecucionDeHijo {
    hijo: Child,
    terminado: bool,
}

impl Ejecucion for EjecucionDeHijo {
    fn terminada(&mut self) -> Option<String> {
        match self.hijo.try_wait() {
            Ok(None) => None,
            Ok(Some(_)) | Err(_) => {
                self.terminado = true;
                let mut salida = String::new();
                if let Some(stdout) = self.hijo.stdout.take() {
                    let _ = stdout.take(64 * 1024).read_to_string(&mut salida);
                }
                Some(salida)
            }
        }
    }

    fn cortar(&mut self) {
        if !self.terminado {
            let _ = self.hijo.kill();
            let _ = self.hijo.wait();
            self.terminado = true;
        }
    }
}

impl Drop for EjecucionDeHijo {
    fn drop(&mut self) {
        self.cortar();
    }
}

/// Un vigía con respuestas fijas: para la demostración y las pruebas.
#[derive(Debug, Clone, Default)]
pub struct VigiaFijo {
    estados: HashMap<String, EstadoServicio>,
}

impl VigiaFijo {
    /// Un vigía que responde `estado` para `unidad`.
    pub fn con(mut self, unidad: &str, estado: EstadoServicio) -> VigiaFijo {
        self.estados.insert(unidad.to_string(), estado);
        self
    }
}

impl Vigia for VigiaFijo {
    fn consultar(&self, unidad: &str) -> EstadoServicio {
        self.estados
            .get(unidad)
            .cloned()
            .unwrap_or(EstadoServicio::NoEncontrado)
    }
}

/// Una lectura del estado de un servicio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lectura {
    /// El estado.
    pub estado: EstadoServicio,
    /// Hace cuántos milisegundos se consultó.
    pub hace_ms: u64,
    /// Si salió de la caché.
    pub de_cache: bool,
}

/// Un vigía con caché de [`VIDA_CACHE`] por unidad.
pub struct VigiaConCache {
    vigia: Box<dyn Vigia>,
    reloj: Arc<dyn Reloj>,
    vida_ms: u64,
    guardadas: Mutex<HashMap<String, (u64, EstadoServicio)>>,
}

impl VigiaConCache {
    /// Envuelve `vigia` con la caché.
    pub fn nueva(vigia: Box<dyn Vigia>, reloj: Arc<dyn Reloj>) -> VigiaConCache {
        VigiaConCache {
            vigia,
            reloj,
            vida_ms: u64::try_from(VIDA_CACHE.as_millis()).unwrap_or(u64::MAX),
            guardadas: Mutex::new(HashMap::new()),
        }
    }

    /// El estado de `unidad`: el guardado si tiene menos de [`VIDA_CACHE`], o una consulta
    /// nueva. La caché queda tomada durante la consulta, así que dos peticiones a la vez
    /// tampoco consultan dos veces.
    pub fn leer(&self, unidad: &str) -> Lectura {
        let mut guardadas = self
            .guardadas
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let ahora = self.reloj.ahora_ms();
        if let Some((cuando, estado)) = guardadas.get(unidad) {
            let edad = ahora.saturating_sub(*cuando);
            if edad < self.vida_ms {
                return Lectura {
                    estado: estado.clone(),
                    hace_ms: edad,
                    de_cache: true,
                };
            }
        }
        let estado = self.vigia.consultar(unidad);
        let cuando = self.reloj.ahora_ms();
        guardadas.insert(unidad.to_string(), (cuando, estado.clone()));
        Lectura {
            estado,
            hace_ms: 0,
            de_cache: false,
        }
    }
}
