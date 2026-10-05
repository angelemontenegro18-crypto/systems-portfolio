//! Protocolo del trabajador de demostración (`trabajador-demo`).
//!
//! Sirve para la demo y para los tests. Además de una tarea útil tiene tareas
//! que se portan mal a propósito — colgarse, inundar la salida, fallar — para
//! poder probar que el coordinador las contiene.

use serde::{Deserialize, Serialize};

use crate::instantanea::Codificable;

/// Configuración compartida que el coordinador publica y cada sesión recibe
/// como copia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigCompartida {
    /// Máximo de elementos por petición.
    pub max_elementos: u32,
    /// Factor por el que se multiplica cada elemento.
    pub factor: i64,
}

impl Codificable<2> for ConfigCompartida {
    fn a_palabras(&self) -> [u64; 2] {
        [
            u64::from(self.max_elementos),
            u64::from_ne_bytes(self.factor.to_ne_bytes()),
        ]
    }
    fn desde_palabras(p: [u64; 2]) -> Self {
        Self {
            max_elementos: u32::try_from(p[0]).unwrap_or(u32::MAX),
            factor: i64::from_ne_bytes(p[1].to_ne_bytes()),
        }
    }
}

/// Qué hace el trabajador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tarea {
    /// Multiplica cada valor por el factor de la configuración.
    Escalar(Vec<i64>),
    /// Duerme y no responde (para probar el plazo).
    Dormir {
        /// Milisegundos.
        ms: u64,
    },
    /// Escribe esa cantidad de bytes en stdout (para probar el tope).
    Inundar {
        /// Bytes.
        bytes: usize,
    },
    /// Termina con ese código y un mensaje en stderr.
    Fallar {
        /// Código de salida.
        codigo: i32,
    },
}

/// Lo que recibe el trabajador por stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peticion {
    /// Generación de la configuración con la que se armó la petición.
    pub generacion: u64,
    /// Copia de la configuración.
    pub config: ConfigCompartida,
    /// La tarea.
    pub tarea: Tarea,
}

/// Lo que devuelve el trabajador por stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Respuesta {
    /// La generación que usó: la sesión trabajó sobre esa configuración y no otra.
    pub generacion: u64,
    /// Resultado.
    pub valores: Vec<i64>,
}
