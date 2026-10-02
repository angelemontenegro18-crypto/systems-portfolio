//! # tool-gateway
//!
//! Registro de herramientas para modelos de lenguaje con *function calling*.
//!
//! Un agente con herramientas es tan seguro como la herramienta más peligrosa
//! que puede llamar. Este crate aplica una regla simple y la hace verificable:
//! **solo funciones puras** — sin E/S, sin estado, sin aleatoriedad.
//!
//! - Las herramientas son punteros a función (`fn`): no pueden capturar estado.
//! - Una [lista de exclusión](politica::EXCLUIDAS) veta categorías enteras
//!   (procesos, disco, red, correo, entorno, azar), y el registro se niega a
//!   aceptarlas.
//! - Los argumentos del modelo se validan contra el esquema **antes** de
//!   llamar, y el esquema no puede usar restricciones que el validador no aplica.
//! - Un error o un pánico de la herramienta vuelve al modelo como resultado
//!   con marca de error; el gateway sigue en pie.
//!
//! ```
//! use serde_json::json;
//!
//! let registro = tool_gateway::catalogo().expect("catálogo válido");
//! let manifiesto = registro.manifiesto(); // se le pasa al modelo
//! assert!(manifiesto.as_array().is_some_and(|m| !m.is_empty()));
//!
//! // El modelo pide una herramienta con argumentos:
//! let r = registro.invocar("distancia", &json!({"a": {"x": 0, "y": 0}, "b": {"x": 3, "y": 4}}));
//! assert_eq!(r, Ok(json!({"distancia": 5.0})));
//! ```
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalogo;
pub mod esquema;
pub mod politica;
pub mod registro;

pub use catalogo::{catalogo, herramientas};
pub use registro::{ErrorInvocacion, ErrorRegistro, Funcion, Herramienta, Limites, Registro};
