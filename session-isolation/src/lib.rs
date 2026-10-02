//! # session-isolation
//!
//! Aislamiento de sesiones por proceso, con tres piezas que se pueden usar
//! por separado:
//!
//! ```text
//!   escritor ──publicar()──▶ ┌─────────────────────┐ ◀──leer()── N lectores
//!                            │ Publicador (seqlock) │   sin locks, sin unsafe
//!                            └──────────┬──────────┘
//!                                       │ copia de la instantánea
//!                                       ▼
//!                  ipc::ejecutar ──stdin──▶ [proceso efímero] ──stdout──▶ respuesta
//!                                 plazo, tope de salida, siempre se espera al hijo
//!
//!   Pool ─ slots con identidad estable ─ reemplazo en caliente (el nuevo arranca
//!          antes de que el viejo termine) ─ Juez con histéresis decide cuándo
//! ```
//!
//! - [`instantanea`]: estado compartido de escritura rara y lectura masiva.
//! - [`ipc`]: una sesión, un proceso; el hijo solo ve una copia.
//! - [`pool`]: procesos residentes que se reemplazan sin perder su slot.
//! - [`salud`]: cuándo reemplazar, sin reaccionar a un pico aislado.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::PathBuf;

pub mod demo;
pub mod instantanea;
pub mod ipc;
pub mod pool;
pub mod salud;

pub use instantanea::{Codificable, Instantanea, Publicador};
pub use ipc::{ejecutar, ErrorIpc, Limites};
pub use pool::{ErrorPool, Evento, MetaSlot, Pool, VistaSlot};
pub use salud::{Juez, Medicion, Umbrales, Veredicto};

/// Identificador estable de un slot del pool.
pub type SlotId = u32;

/// Cómo lanzar un proceso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receta {
    /// Ejecutable.
    pub programa: PathBuf,
    /// Argumentos.
    pub args: Vec<String>,
}

impl Receta {
    /// Una receta con argumentos.
    pub fn nueva(programa: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self { programa: programa.into(), args: args.iter().map(|a| a.to_string()).collect() }
    }
}
