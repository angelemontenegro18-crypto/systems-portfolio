//! La API de la consola y su interfaz: qué responde cada ruta.
//!
//! | Ruta | Método | Qué hace |
//! |---|---|---|
//! | `/`, `/app.css`, `/app.js`, `/icono.svg` | GET | la interfaz, incrustada en el binario |
//! | `/api/estado` | GET | la consola y el grupo: política, posiciones, último tick |
//! | `/api/grupo/reciclar` | POST | `{"posicion": id}`: recicla esa posición |
//! | `/api/grupo/escala` | POST | `{"cantidad": n}`, `{"min": a, "max": b}` o los tres |
//! | `/api/grupo/tick` | POST | `{}`: revisa el grupo ahora |
//! | `/api/servicios` | GET | el estado de cada servicio vigilado |
//! | `/api/registros?n=…` | GET | las últimas `n` líneas del registro, redactadas |
//! | `/api/sistema` | GET | qué variables de configuración están definidas (sí o no) |
//!
//! Toda respuesta de la API es JSON con la misma forma: `{"ok": true, "datos": …}` o
//! `{"ok": false, "error": {"estado", "codigo", "mensaje"}}`. Un cuerpo con campos que la
//! ruta no espera se rechaza entero (400): nada se interpreta a medias.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::estaticos;
use crate::grupo::{Accion, ErrorGrupo, Grupo, Politica, VistaPosicion, TOPE_DE_POSICIONES};
use crate::http::{Metodo, Peticion, Respuesta};
use crate::json::{self, Json};
use crate::registros::{self, Redactado, MARCA};
use crate::reloj::Reloj;
use crate::servidor::Manejador;
use crate::sistema::{self, Entorno};
use crate::vigia::{Vigia, VigiaConCache, LIMITE, VIDA_CACHE};

/// Líneas de registro que se devuelven si no se pide otra cantidad.
pub const LINEAS_POR_DEFECTO: usize = 100;

/// Líneas de registro que se pueden pedir, como máximo.
pub const LINEAS_MAX: usize = 500;

/// Bytes del final del registro que se miran, como máximo.
pub const TOPE_REGISTROS: u64 = 256 * 1024;

/// Las piezas con las que se arma una consola.
pub struct Piezas {
    /// El grupo de trabajadores, ya lanzado.
    pub grupo: Grupo,
    /// De dónde sale el estado de los servicios.
    pub vigia: Box<dyn Vigia>,
    /// Las unidades a vigilar.
    pub servicios: Vec<String>,
    /// El archivo de registro.
    pub registros: PathBuf,
    /// De dónde sale si cada variable está definida.
    pub entorno: Box<dyn Entorno>,
    /// El reloj.
    pub reloj: Arc<dyn Reloj>,
}

struct UltimoTick {
    cuando_ms: u64,
    origen: &'static str,
    acciones: Vec<Accion>,
}

/// La consola: atiende la API sobre un grupo, un vigía de servicios, un registro y un entorno.
pub struct Consola {
    grupo: Mutex<Grupo>,
    ultimo_tick: Mutex<Option<UltimoTick>>,
    vigia: VigiaConCache,
    servicios: Vec<String>,
    registros: PathBuf,
    entorno: Box<dyn Entorno>,
    reloj: Arc<dyn Reloj>,
    arranque_ms: u64,
}

/// Un error de la API: estado HTTP, código estable y explicación.
struct Fallo {
    estado: u16,
    codigo: &'static str,
    mensaje: String,
}

impl Fallo {
    fn nuevo(estado: u16, codigo: &'static str, mensaje: impl Into<String>) -> Fallo {
        Fallo {
            estado,
            codigo,
            mensaje: mensaje.into(),
        }
    }
}

impl From<ErrorGrupo> for Fallo {
    fn from(error: ErrorGrupo) -> Fallo {
        let (estado, codigo) = match error {
            ErrorGrupo::PoliticaInvalida(_) => (400, "politica_invalida"),
            ErrorGrupo::FueraDePolitica { .. } => (400, "fuera_de_politica"),
            ErrorGrupo::PosicionDesconocida(_) => (404, "posicion_desconocida"),
            ErrorGrupo::Lanzar(_) => (500, "fallo_al_lanzar"),
        };
        Fallo::nuevo(estado, codigo, error.to_string())
    }
}

impl Consola {
    /// Arma la consola.
    pub fn nueva(piezas: Piezas) -> Consola {
        let arranque_ms = piezas.reloj.ahora_ms();
        Consola {
            grupo: Mutex::new(piezas.grupo),
            ultimo_tick: Mutex::new(None),
            vigia: VigiaConCache::nueva(piezas.vigia, Arc::clone(&piezas.reloj)),
            servicios: piezas.servicios,
            registros: piezas.registros,
            entorno: piezas.entorno,
            reloj: piezas.reloj,
            arranque_ms,
        }
    }

    /// Revisa el grupo, como `POST /api/grupo/tick`. El binario lo llama solo, cada tanto.
    pub fn tick(&self) -> Vec<Accion> {
        self.revisar("automatico")
    }

    fn revisar(&self, origen: &'static str) -> Vec<Accion> {
        let acciones = self.grupo().tick();
        *self
            .ultimo_tick
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(UltimoTick {
            cuando_ms: self.reloj.ahora_ms(),
            origen,
            acciones: acciones.clone(),
        });
        acciones
    }

    fn grupo(&self) -> MutexGuard<'_, Grupo> {
        self.grupo.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn estado(&self) -> Json {
        let vista = self.grupo().vista();
        let ahora = self.reloj.ahora_ms();
        let ultimo = match &*self
            .ultimo_tick
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            None => Json::Nulo,
            Some(t) => Json::objeto([
                ("hace_ms", ahora.saturating_sub(t.cuando_ms).into()),
                ("origen", t.origen.into()),
                (
                    "acciones",
                    t.acciones
                        .iter()
                        .map(json_accion)
                        .collect::<Vec<_>>()
                        .into(),
                ),
            ]),
        };
        Json::objeto([
            (
                "consola",
                Json::objeto([
                    ("version", env!("CARGO_PKG_VERSION").into()),
                    (
                        "plataforma",
                        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH).into(),
                    ),
                    ("activa_ms", ahora.saturating_sub(self.arranque_ms).into()),
                ]),
            ),
            ("politica", json_politica(vista.politica)),
            (
                "posiciones",
                vista
                    .posiciones
                    .iter()
                    .map(json_posicion)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            ("ultimo_tick", ultimo),
        ])
    }

    fn reciclar(&self, cuerpo: &[u8]) -> Result<Json, Fallo> {
        let valor = objeto_json(cuerpo, &["posicion"])?;
        let id = entero(&valor, "posicion")?
            .ok_or_else(|| Fallo::nuevo(400, "falta_campo", "falta «posicion»"))?;
        let id = u32::try_from(id)
            .map_err(|_| Fallo::nuevo(400, "valor_invalido", "«posicion» no es un id válido"))?;
        let accion = self.grupo().reciclar(id)?;
        Ok(json_accion(&accion))
    }

    /// Cambia la política, la cantidad o las dos. Primero valida todo y recién después toca
    /// el grupo: o se aplica entero o no se aplica nada.
    fn escala(&self, cuerpo: &[u8]) -> Result<Json, Fallo> {
        let valor = objeto_json(cuerpo, &["cantidad", "min", "max"])?;
        let cantidad = entero(&valor, "cantidad")?
            .map(|n| no_negativo(n, "cantidad"))
            .transpose()?;
        let limites = match (entero(&valor, "min")?, entero(&valor, "max")?) {
            (None, None) => None,
            (Some(min), Some(max)) => Some((no_negativo(min, "min")?, no_negativo(max, "max")?)),
            _ => {
                return Err(Fallo::nuevo(
                    400,
                    "politica_incompleta",
                    "«min» y «max» se mandan juntos",
                ))
            }
        };
        if cantidad.is_none() && limites.is_none() {
            return Err(Fallo::nuevo(
                400,
                "falta_campo",
                "se espera «cantidad», o «min» y «max»",
            ));
        }

        let mut grupo = self.grupo();
        let politica = match limites {
            Some((min, max)) => Politica::nueva(min, max, grupo.politica().vida_maxima())?,
            None => grupo.politica(),
        };
        if let Some(cantidad) = cantidad {
            if cantidad < politica.min() || cantidad > politica.max() {
                return Err(ErrorGrupo::FueraDePolitica {
                    cantidad,
                    min: politica.min(),
                    max: politica.max(),
                }
                .into());
            }
        }
        let antes = grupo.cantidad();
        if let Some((min, max)) = limites {
            grupo.cambiar_politica(min, max)?;
        }
        if let Some(cantidad) = cantidad {
            grupo.escalar(cantidad)?;
        }
        Ok(Json::objeto([
            ("antes", antes.into()),
            ("despues", grupo.cantidad().into()),
            ("politica", json_politica(grupo.politica())),
        ]))
    }

    fn tick_manual(&self, cuerpo: &[u8]) -> Result<Json, Fallo> {
        objeto_json(cuerpo, &[])?;
        let acciones = self.revisar("manual");
        Ok(Json::objeto([(
            "acciones",
            acciones.iter().map(json_accion).collect::<Vec<_>>().into(),
        )]))
    }

    fn servicios(&self) -> Json {
        let servicios = self
            .servicios
            .iter()
            .map(|unidad| {
                let lectura = self.vigia.leer(unidad);
                Json::objeto([
                    ("unidad", unidad.as_str().into()),
                    ("estado", lectura.estado.clave().into()),
                    ("detalle", lectura.estado.detalle().into()),
                    ("hace_ms", lectura.hace_ms.into()),
                    ("de_cache", lectura.de_cache.into()),
                ])
            })
            .collect::<Vec<_>>();
        Json::objeto([
            ("servicios", servicios.into()),
            ("limite_s", LIMITE.as_secs().into()),
            ("cache_s", VIDA_CACHE.as_secs().into()),
        ])
    }

    fn registros(&self, peticion: &Peticion) -> Result<Json, Fallo> {
        let lineas = match peticion.parametro("n") {
            None => LINEAS_POR_DEFECTO,
            Some(texto) => texto
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=LINEAS_MAX).contains(n))
                .ok_or_else(|| {
                    Fallo::nuevo(
                        400,
                        "parametro_invalido",
                        format!("«n» tiene que ser un entero entre 1 y {LINEAS_MAX}"),
                    )
                })?,
        };
        let cola = registros::leer_cola(&self.registros, lineas, TOPE_REGISTROS).map_err(|e| {
            Fallo::nuevo(
                503,
                "registro_no_disponible",
                format!("no se pudo leer el registro: {}", motivo_de_lectura(&e)),
            )
        })?;
        let nombre = self
            .registros
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Json::objeto([
            ("archivo", texto_redactado(&registros::redactar(&nombre))),
            (
                "lineas",
                cola.lineas
                    .iter()
                    .map(texto_redactado)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            (
                "redacciones",
                cola.lineas
                    .iter()
                    .map(|l| l.como_str().matches(MARCA).count())
                    .sum::<usize>()
                    .into(),
            ),
            ("recortado", cola.recortado.into()),
            ("bytes_leidos", cola.bytes_leidos.into()),
            ("bytes_archivo", cola.bytes_archivo.into()),
            ("tope", TOPE_REGISTROS.into()),
        ]))
    }

    fn sistema(&self) -> Json {
        let variables = sistema::presencia(self.entorno.as_ref())
            .into_iter()
            .map(|(nombre, definida)| (nombre, Json::Bool(definida)));
        Json::objeto([("variables", Json::objeto(variables))])
    }
}

impl Manejador for Consola {
    fn atender(&self, peticion: &Peticion) -> Respuesta {
        let ruta = peticion.cabeza.ruta.as_str();
        let metodo = peticion.cabeza.metodo;
        if let Some(recurso) = estaticos::buscar(ruta) {
            return match metodo {
                Metodo::Get => Respuesta::estatico(recurso.tipo, recurso.contenido),
                Metodo::Post => metodo_no_permitido("GET"),
            };
        }
        let resultado = match (metodo, ruta) {
            (Metodo::Get, "/api/estado") => Ok(self.estado()),
            (Metodo::Post, "/api/grupo/reciclar") => self.reciclar(&peticion.cuerpo),
            (Metodo::Post, "/api/grupo/escala") => self.escala(&peticion.cuerpo),
            (Metodo::Post, "/api/grupo/tick") => self.tick_manual(&peticion.cuerpo),
            (Metodo::Get, "/api/servicios") => Ok(self.servicios()),
            (Metodo::Get, "/api/registros") => self.registros(peticion),
            (Metodo::Get, "/api/sistema") => Ok(self.sistema()),
            (_, "/api/estado" | "/api/servicios" | "/api/registros" | "/api/sistema") => {
                return metodo_no_permitido("GET")
            }
            (_, "/api/grupo/reciclar" | "/api/grupo/escala" | "/api/grupo/tick") => {
                return metodo_no_permitido("POST")
            }
            _ => Err(Fallo::nuevo(404, "ruta_desconocida", "esa ruta no existe")),
        };
        match resultado {
            Ok(datos) => Respuesta::ok(datos),
            Err(fallo) => Respuesta::error(fallo.estado, fallo.codigo, fallo.mensaje),
        }
    }
}

/// El único camino de un texto de registro hacia el JSON: desde un [`Redactado`].
fn texto_redactado(texto: &Redactado) -> Json {
    Json::from(texto.como_str())
}

fn metodo_no_permitido(permitido: &'static str) -> Respuesta {
    Respuesta::error(
        405,
        "metodo_no_permitido",
        format!("esta ruta solo acepta {permitido}"),
    )
    .con_cabecera("Allow", permitido)
}

/// El cuerpo como objeto JSON, sin campos fuera de `permitidos`.
fn objeto_json(cuerpo: &[u8], permitidos: &[&str]) -> Result<Json, Fallo> {
    let texto = std::str::from_utf8(cuerpo)
        .map_err(|_| Fallo::nuevo(400, "json_invalido", "el cuerpo no es UTF-8"))?;
    let valor =
        json::analizar(texto).map_err(|e| Fallo::nuevo(400, "json_invalido", e.to_string()))?;
    let Json::Objeto(pares) = &valor else {
        return Err(Fallo::nuevo(
            400,
            "json_invalido",
            "el cuerpo tiene que ser un objeto JSON",
        ));
    };
    if let Some((clave, _)) = pares
        .iter()
        .find(|(k, _)| !permitidos.contains(&k.as_str()))
    {
        return Err(Fallo::nuevo(
            400,
            "campo_desconocido",
            format!("campo desconocido: «{clave}»"),
        ));
    }
    Ok(valor)
}

/// El entero de `clave`, si vino; si vino con otro tipo, un error.
fn entero(valor: &Json, clave: &str) -> Result<Option<i64>, Fallo> {
    match valor.campo(clave) {
        None => Ok(None),
        Some(campo) => campo.como_entero().map(Some).ok_or_else(|| {
            Fallo::nuevo(
                400,
                "tipo_invalido",
                format!("«{clave}» tiene que ser un número entero"),
            )
        }),
    }
}

fn no_negativo(n: i64, clave: &str) -> Result<usize, Fallo> {
    usize::try_from(n).map_err(|_| {
        Fallo::nuevo(
            400,
            "valor_invalido",
            format!("«{clave}» no puede ser negativo"),
        )
    })
}

fn motivo_de_lectura(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "el archivo no existe",
        io::ErrorKind::PermissionDenied => "no hay permiso para leerlo",
        _ => "error de lectura",
    }
}

fn json_politica(politica: Politica) -> Json {
    Json::objeto([
        ("min", politica.min().into()),
        ("max", politica.max().into()),
        ("vida_s", politica.vida_maxima().as_secs().into()),
        ("tope", TOPE_DE_POSICIONES.into()),
    ])
}

fn json_posicion(p: &VistaPosicion) -> Json {
    Json::objeto([
        ("id", p.id.into()),
        ("pid", p.pid.into()),
        ("estado", p.estado.nombre().into()),
        ("hace_ms", p.hace_ms.into()),
        ("edad_ms", p.edad_ms.into()),
        ("generacion", p.generacion.into()),
    ])
}

fn json_accion(a: &Accion) -> Json {
    let (pid_nuevo, error) = match &a.resultado {
        Ok(pid) => (Json::from(*pid), Json::Nulo),
        Err(motivo) => (Json::Nulo, Json::from(motivo.as_str())),
    };
    Json::objeto([
        ("posicion", a.posicion.into()),
        ("motivo", a.motivo.nombre().into()),
        ("pid_anterior", a.pid_anterior.into()),
        ("pid_nuevo", pid_nuevo),
        ("error", error),
    ])
}
