//! El registro de herramientas y su despacho.
//!
//! Cada herramienta es un **puntero a función** (`fn`), no un closure. Un `fn`
//! no puede capturar estado, así que "sin estado" queda garantizado por el tipo
//! y no por la disciplina de quien registra.
//!
//! El despacho nunca entra en pánico hacia afuera: argumentos inválidos, una
//! herramienta que falla o una que entra en pánico se devuelven como errores
//! estructurados, listos para volver al modelo como resultado de la llamada.

use std::collections::BTreeMap;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::{json, Value};

use crate::esquema;
use crate::politica;

/// La firma de toda herramienta: argumentos ya validados → resultado.
pub type Funcion = fn(&Value) -> Result<Value, String>;

/// Una herramienta expuesta al modelo.
#[derive(Debug, Clone)]
pub struct Herramienta {
    /// Nombre con el que el modelo la llama: minúsculas, dígitos y `_`.
    pub nombre: &'static str,
    /// Qué hace, en una frase que el modelo pueda usar para decidir.
    pub descripcion: &'static str,
    /// Esquema de entrada (subconjunto de JSON Schema).
    pub esquema: Value,
    /// Una entrada válida de ejemplo: documenta y alimenta los tests.
    pub ejemplo: Value,
    /// La implementación.
    pub funcion: Funcion,
}

/// Por qué no se pudo registrar una herramienta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorRegistro {
    /// El nombre está en la lista de exclusión.
    Vetada {
        /// Nombre.
        nombre: String,
        /// Motivo del veto.
        motivo: &'static str,
    },
    /// Ya hay una herramienta con ese nombre.
    Duplicada(String),
    /// El nombre no es un identificador válido.
    NombreInvalido(String),
    /// El esquema usa algo que el validador no aplica.
    EsquemaInvalido {
        /// Herramienta.
        nombre: String,
        /// Detalle.
        detalle: String,
    },
    /// El ejemplo no cumple su propio esquema.
    EjemploInvalido {
        /// Herramienta.
        nombre: String,
        /// Problemas encontrados.
        errores: Vec<String>,
    },
}

impl fmt::Display for ErrorRegistro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vetada { nombre, motivo } => write!(f, "`{nombre}` está vetada: {motivo}"),
            Self::Duplicada(n) => write!(f, "ya hay una herramienta `{n}`"),
            Self::NombreInvalido(n) => write!(
                f,
                "`{n}` no es un nombre válido (minúsculas, dígitos y `_`)"
            ),
            Self::EsquemaInvalido { nombre, detalle } => {
                write!(f, "esquema de `{nombre}`: {detalle}")
            }
            Self::EjemploInvalido { nombre, errores } => {
                write!(
                    f,
                    "el ejemplo de `{nombre}` no cumple su esquema: {}",
                    errores.join("; ")
                )
            }
        }
    }
}

impl std::error::Error for ErrorRegistro {}

/// Por qué falló una invocación. Se devuelve al modelo, nunca se propaga como pánico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorInvocacion {
    /// No hay ninguna herramienta con ese nombre.
    NoExiste(String),
    /// Los argumentos no cumplen el esquema.
    ArgumentosInvalidos(Vec<String>),
    /// Los argumentos superan el tamaño máximo.
    EntradaDemasiadoGrande {
        /// Bytes recibidos.
        bytes: usize,
        /// Máximo.
        maximo: usize,
    },
    /// El resultado supera el tamaño máximo.
    SalidaDemasiadoGrande {
        /// Bytes producidos.
        bytes: usize,
        /// Máximo.
        maximo: usize,
    },
    /// La herramienta devolvió un error.
    Fallo(String),
    /// La herramienta entró en pánico; el gateway siguió en pie.
    Panico,
}

impl fmt::Display for ErrorInvocacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoExiste(n) => write!(f, "no existe la herramienta `{n}`"),
            Self::ArgumentosInvalidos(e) => write!(f, "argumentos inválidos: {}", e.join("; ")),
            Self::EntradaDemasiadoGrande { bytes, maximo } => {
                write!(f, "argumentos de {bytes} bytes (máximo {maximo})")
            }
            Self::SalidaDemasiadoGrande { bytes, maximo } => {
                write!(f, "resultado de {bytes} bytes (máximo {maximo})")
            }
            Self::Fallo(m) => write!(f, "la herramienta falló: {m}"),
            Self::Panico => write!(f, "la herramienta falló de forma inesperada"),
        }
    }
}

impl std::error::Error for ErrorInvocacion {}

/// Límites de tamaño de cada invocación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    /// Máximo de bytes de los argumentos, serializados.
    pub max_bytes_entrada: usize,
    /// Máximo de bytes del resultado, serializado.
    pub max_bytes_salida: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            max_bytes_entrada: 64 * 1024,
            max_bytes_salida: 64 * 1024,
        }
    }
}

/// Registro de herramientas. Ordenado por nombre para que el manifiesto sea estable.
#[derive(Debug, Clone, Default)]
pub struct Registro {
    herramientas: BTreeMap<&'static str, Herramienta>,
    limites: Limites,
}

fn nombre_valido(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && n.starts_with(|c: char| c.is_ascii_lowercase())
        && n.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Registro {
    /// Un registro vacío con límites por defecto.
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Un registro vacío con estos límites.
    pub fn con_limites(limites: Limites) -> Self {
        Self {
            herramientas: BTreeMap::new(),
            limites,
        }
    }

    /// Registra una herramienta, después de verificar nombre, veto, esquema y ejemplo.
    pub fn registrar(&mut self, h: Herramienta) -> Result<(), ErrorRegistro> {
        if !nombre_valido(h.nombre) {
            return Err(ErrorRegistro::NombreInvalido(h.nombre.to_string()));
        }
        if let Some(motivo) = politica::motivo_de_veto(h.nombre) {
            return Err(ErrorRegistro::Vetada {
                nombre: h.nombre.to_string(),
                motivo,
            });
        }
        if self.herramientas.contains_key(h.nombre) {
            return Err(ErrorRegistro::Duplicada(h.nombre.to_string()));
        }
        esquema::revisar_esquema(&h.esquema).map_err(|detalle| ErrorRegistro::EsquemaInvalido {
            nombre: h.nombre.to_string(),
            detalle,
        })?;
        esquema::validar(&h.esquema, &h.ejemplo).map_err(|errores| {
            ErrorRegistro::EjemploInvalido {
                nombre: h.nombre.to_string(),
                errores,
            }
        })?;
        self.herramientas.insert(h.nombre, h);
        Ok(())
    }

    /// Nombres registrados, en orden.
    pub fn nombres(&self) -> Vec<&'static str> {
        self.herramientas.keys().copied().collect()
    }

    /// Las herramientas registradas, en orden.
    pub fn herramientas(&self) -> impl Iterator<Item = &Herramienta> {
        self.herramientas.values()
    }

    /// Cuántas herramientas hay.
    pub fn len(&self) -> usize {
        self.herramientas.len()
    }

    /// `true` si no hay ninguna.
    pub fn is_empty(&self) -> bool {
        self.herramientas.is_empty()
    }

    /// El manifiesto en el formato habitual de *function calling*:
    /// `[{ "name", "description", "input_schema" }]`.
    pub fn manifiesto(&self) -> Value {
        Value::Array(
            self.herramientas
                .values()
                .map(|h| json!({"name": h.nombre, "description": h.descripcion, "input_schema": h.esquema}))
                .collect(),
        )
    }

    /// Invoca una herramienta con argumentos producidos por el modelo.
    pub fn invocar(&self, nombre: &str, argumentos: &Value) -> Result<Value, ErrorInvocacion> {
        let h = self
            .herramientas
            .get(nombre)
            .ok_or_else(|| ErrorInvocacion::NoExiste(nombre.to_string()))?;

        let bytes = argumentos.to_string().len();
        if bytes > self.limites.max_bytes_entrada {
            return Err(ErrorInvocacion::EntradaDemasiadoGrande {
                bytes,
                maximo: self.limites.max_bytes_entrada,
            });
        }
        esquema::validar(&h.esquema, argumentos).map_err(ErrorInvocacion::ArgumentosInvalidos)?;

        // Una herramienta que entra en pánico no puede tumbar el gateway ni al agente.
        let resultado = catch_unwind(AssertUnwindSafe(|| (h.funcion)(argumentos)))
            .map_err(|_| ErrorInvocacion::Panico)?
            .map_err(ErrorInvocacion::Fallo)?;

        let bytes = resultado.to_string().len();
        if bytes > self.limites.max_bytes_salida {
            return Err(ErrorInvocacion::SalidaDemasiadoGrande {
                bytes,
                maximo: self.limites.max_bytes_salida,
            });
        }
        Ok(resultado)
    }

    /// Invoca y arma el bloque de resultado para devolverle al modelo, con la
    /// marca de error cuando corresponde. Nunca falla.
    pub fn responder(&self, id_llamada: &str, nombre: &str, argumentos: &Value) -> Value {
        match self.invocar(nombre, argumentos) {
            Ok(v) => {
                json!({"tool_use_id": id_llamada, "content": v.to_string(), "is_error": false})
            }
            Err(e) => {
                json!({"tool_use_id": id_llamada, "content": e.to_string(), "is_error": true})
            }
        }
    }
}
