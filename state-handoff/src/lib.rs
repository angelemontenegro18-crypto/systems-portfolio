//! # state-handoff
//!
//! Traspaso de estado entre un controlador activo y su respaldo, en `no_std` y sin `alloc`.
//! El activo sella el estado —aquí, la tabla de calibración de una bomba— en un paquete con
//! cabecera propia y dos CRC-32; el respaldo lo verifica, lo pasa por una **cerca de época** y
//! lo confirma o lo aborta. Sirve igual en memoria compartida que por un enlace: el paquete es
//! una tira de bytes que se explica sola.
//!
//! - [`crc32`]: CRC-32 de IEEE 802.3, con la tabla calculada en compilación.
//! - [`le`]: escritura y lectura little-endian explícitas, sin `unwrap` ni indexación.
//! - [`cabecera`]: el formato de la cabecera, con su largo atado a sus campos en compilación.
//! - [`paquete`]: sellar, y verificar por tipos: `Paquete<Sellado>` → `Paquete<Verificado>`.
//! - [`receptor`]: la cerca de época y la confirmación en dos fases, con borrado al confirmar.
//!
//! ```
//! use state_handoff::paquete::sellar;
//! use state_handoff::receptor::Receptor;
//!
//! // El activo (nodo 1) sella la tabla para el respaldo (nodo 2), en la época 7.
//! let tabla = [10u8, 20, 30, 40];
//! let mut bytes = [0u8; 64];
//! let n = sellar(&tabla, 1, 2, 7, &mut bytes)?;
//!
//! // El respaldo la carga, la prepara (verifica y pasa la cerca), la usa y confirma.
//! let mut respaldo = Receptor::<64>::nuevo(2);
//! respaldo.cargar(&bytes[..n])?;
//! let preparado = respaldo.preparar()?;
//! assert_eq!(preparado.contenido(), &tabla);
//! preparado.confirmar();
//! assert_eq!(respaldo.ultima_epoca(), 7);
//!
//! // El mismo paquete otra vez es una repetición: la cerca lo rechaza.
//! respaldo.cargar(&bytes[..n])?;
//! assert!(respaldo.preparar().is_err());
//! # Ok::<(), state_handoff::Error>(())
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod cabecera;
pub mod crc32;
pub mod error;
pub mod le;
pub mod paquete;
pub mod receptor;

pub use error::Error;
