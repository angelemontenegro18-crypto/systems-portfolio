//! # spread-spectrum-codec
//!
//! Un módem de **espectro ensanchado por secuencia directa** (DSSS), `no_std` y con
//! aritmética entera, para transmitir lecturas de sensores por un enlace con ruido y con un
//! interferente de banda estrecha. Es la técnica de GPS, CDMA y 802.15.4: cada bit se
//! transmite multiplicado por una secuencia de `N` chips, y el receptor correla con esa misma
//! secuencia. La señal se suma en fase; lo que no se le parece —un tono, el ruido— se reparte.
//!
//! - [`lfsr`]: secuencias de máximo largo, de grado 3 a 7, con polinomios primitivos de tabla.
//! - [`gold`]: códigos Gold a partir de un par preferente.
//! - [`ensanchado`]: esparcir y desesparcir, con decisión dura y blanda.
//! - [`hamming`]: Hamming ampliado (8,4), que corrige un error y detecta dos por palabra.
//! - [`entrelazado`]: un entrelazador de bloques, que convierte una ráfaga en errores aislados.
//! - [`sincronia`]: sincronización de trama por correlación contra un preámbulo.
//!
//! Sin `alloc` y sin dependencias. Ninguna función entra en pánico: los errores vuelven como
//! [`Error`].
//!
//! ```
//! use spread_spectrum_codec::ensanchado::{decidir, esparcir};
//! use spread_spectrum_codec::hamming::{codificar, decodificar, Decodificacion};
//! use spread_spectrum_codec::lfsr::{Codigo, Polinomio};
//!
//! // Una lectura de 4 bits, protegida con Hamming (8,4).
//! let palabra = codificar(0b1011).unwrap();
//! let bits: Vec<u8> = (0..8).map(|j| (palabra >> j) & 1).collect();
//!
//! // Cada bit, esparcido con una secuencia de 31 chips.
//! let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);
//! let mut muestras = vec![0i32; bits.len() * codigo.largo()];
//! esparcir(&bits, &codigo, 100, &mut muestras).unwrap();
//!
//! // Un interferente constante, más fuerte que cada chip, no alcanza a voltear un bit.
//! for m in muestras.iter_mut() {
//!     *m += 140;
//! }
//!
//! let mut recibidos = vec![0u8; bits.len()];
//! decidir(&muestras, &codigo, &mut recibidos).unwrap();
//! let palabra = recibidos.iter().enumerate().fold(0u8, |p, (j, b)| p | (b << j));
//! assert_eq!(decodificar(palabra), Decodificacion::Intacta(0b1011));
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

pub mod ensanchado;
pub mod entrelazado;
pub mod error;
pub mod gold;
pub mod hamming;
pub mod lfsr;
pub mod sincronia;

pub use error::Error;
