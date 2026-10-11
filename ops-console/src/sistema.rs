//! Qué variables de configuración están definidas — **nunca sus valores**.
//!
//! La garantía está en la forma del rasgo: un [`Entorno`] solo sabe responder si una variable
//! está definida. La API no tiene de dónde sacar un valor aunque quisiera.

use std::collections::HashMap;

/// Las variables que lee el servicio de miniaturas.
pub const VARIABLES: [&str; 6] = [
    "MINIATURAS_CLAVE_API",
    "MINIATURAS_TOKEN_CDN",
    "MINIATURAS_URL_ORIGEN",
    "MINIATURAS_DIR_CACHE",
    "MINIATURAS_NIVEL_REGISTRO",
    "MINIATURAS_CLAVE_FIRMA",
];

/// Algo que sabe si una variable está definida.
pub trait Entorno: Send + Sync {
    /// Si `nombre` está definida y no vacía.
    fn definida(&self, nombre: &str) -> bool;
}

/// El entorno del propio proceso.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntornoDelProceso;

impl Entorno for EntornoDelProceso {
    fn definida(&self, nombre: &str) -> bool {
        std::env::var_os(nombre).is_some_and(|valor| !valor.is_empty())
    }
}

/// Un entorno con valores fijos: para la demostración y las pruebas.
#[derive(Debug, Clone, Default)]
pub struct EntornoFijo {
    valores: HashMap<String, String>,
}

impl EntornoFijo {
    /// Un entorno con `nombre` = `valor`.
    pub fn con(mut self, nombre: &str, valor: &str) -> EntornoFijo {
        self.valores.insert(nombre.to_string(), valor.to_string());
        self
    }
}

impl Entorno for EntornoFijo {
    fn definida(&self, nombre: &str) -> bool {
        self.valores
            .get(nombre)
            .is_some_and(|valor| !valor.is_empty())
    }
}

/// Cada variable de [`VARIABLES`], en orden, con si está definida.
pub fn presencia(entorno: &dyn Entorno) -> Vec<(&'static str, bool)> {
    VARIABLES
        .iter()
        .map(|&nombre| (nombre, entorno.definida(nombre)))
        .collect()
}
