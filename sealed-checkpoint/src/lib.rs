//! # sealed-checkpoint
//!
//! Checkpoints de estado sellados con XChaCha20-Poly1305 y guardados de forma
//! atómica y durable.
//!
//! - **Sellado autenticado.** Los datos van cifrados; el encabezado (formato,
//!   versión, generación) va en claro pero autenticado, y el sello queda ligado
//!   al nombre del checkpoint: un archivo renombrado no abre.
//! - **Escritura que sobrevive a un corte.** Temporal nuevo, `sync_all`,
//!   `rename` atómico y `sync_all` del directorio.
//! - **Protección contra retroceso.** Las generaciones solo avanzan, y al
//!   cargar se puede exigir una generación mínima.
//! - **Secretos que no quedan en memoria.** La clave y los datos abiertos se
//!   borran al soltarse, y su `Debug` no los muestra.
//!
//! ```
//! use sealed_checkpoint::{Almacen, Clave};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let dir = std::env::temp_dir().join(format!("sck-doc-{}", std::process::id()));
//! let almacen = Almacen::abrir(&dir, Clave::generar()?)?;
//! almacen.guardar("sesion", 1, b"estado v1")?;
//! almacen.guardar("sesion", 2, b"estado v2")?;
//!
//! let ultimo = almacen.cargar("sesion")?.expect("hay un checkpoint");
//! assert_eq!(ultimo.generacion, 2);
//! assert_eq!(ultimo.datos.as_slice(), b"estado v2");
//! # std::fs::remove_dir_all(&dir)?;
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod almacen;
mod clave;
pub mod sello;

pub use almacen::{Almacen, ErrorAlmacen};
pub use clave::Clave;
pub use sello::{Abierto, ErrorSello};
