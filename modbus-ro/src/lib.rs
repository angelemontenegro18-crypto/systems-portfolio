//! # modbus-ro
//!
//! Cliente Modbus TCP de **solo lectura**, para paneles de monitoreo que tienen
//! que mirar equipos industriales sin ninguna posibilidad de cambiarlos.
//!
//! La garantía no descansa en la disciplina de quien lo usa, sino en el tipo:
//!
//! - [`Funcion`] tiene exactamente dos variantes, ambas de lectura. No hay API
//!   que acepte un código de función arbitrario ni que envíe un PDU crudo.
//! - El crate no tiene dependencias: ninguna librería puede aportar un camino
//!   de escritura.
//! - Un test recorre el propio código fuente y falla si aparece un código de
//!   función de escritura o el nombre de una operación de escritura.
//!
//! ```no_run
//! use std::time::Duration;
//! use modbus_ro::{Cliente, Funcion};
//!
//! # fn main() -> Result<(), modbus_ro::ErrorModbus> {
//! let direccion = "192.0.2.10:502".parse().expect("dirección válida");
//! let mut cliente = Cliente::conectar(direccion, 1, Duration::from_secs(2), Duration::from_secs(1))?;
//! let valores = cliente.leer(Funcion::RegistrosRetencion, 0, 4)?;
//! println!("{valores:?}");
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cliente;
pub mod sondeo;
pub mod trama;

pub use cliente::{Cliente, ErrorModbus};
pub use sondeo::{Bloque, Equipo, EstadoEquipo, Lectura, ResultadoBloque, Sondeador};
pub use trama::{ErrorTrama, Funcion, Peticion};
