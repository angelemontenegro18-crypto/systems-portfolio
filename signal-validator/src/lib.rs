//! # signal-validator
//!
//! Herramientas para no engañarse al validar una señal en una serie temporal.
//!
//! - **Partición con purga y embargo** ([`particion`]): ninguna observación de
//!   entrenamiento comparte datos con las de validación, y las que vienen justo
//!   después del bloque de validación se apartan.
//! - **Permutación por bloques** ([`permutacion`]): la referencia del azar
//!   conserva la dependencia de la serie, y el p-valor nunca es cero.
//! - **Benjamini–Hochberg** ([`fdr`]): con muchas hipótesis, controla la
//!   proporción esperada de descubrimientos falsos.
//! - **Muestra sellada** ([`sellado`]): la reserva final se abre una sola vez
//!   —lo impone el sistema de tipos— y queda comprometida con un SHA-256.
//!
//! Sin dependencias. Los datos de la demo y de los tests son sintéticos y
//! salen de un generador con semilla fija ([`azar`], [`sintetico`]).
//!
//! ```
//! use signal_validator::azar::Generador;
//! use signal_validator::fdr::benjamini_hochberg;
//! use signal_validator::permutacion::{correlacion, prueba_por_bloques};
//! use signal_validator::sellado::MuestraSellada;
//! use signal_validator::sintetico::senal_plantada;
//!
//! let mut g = Generador::nuevo(2026);
//! let escenario = senal_plantada(1500, 5, 2, 0.3, &mut g);
//!
//! // Reservar el último tercio antes de mirar nada.
//! let reserva: Vec<(f64, f64)> = escenario.candidatas[2][1000..]
//!     .iter()
//!     .copied()
//!     .zip(escenario.etiqueta[1000..].iter().copied())
//!     .collect();
//! let reserva = MuestraSellada::sellar(reserva);
//! println!("compromiso: {}", reserva.compromiso());
//!
//! // Diseño: una prueba por candidata, y Benjamini–Hochberg sobre todas.
//! let dos_colas = |x: &[f64], y: &[f64]| correlacion(x, y).abs();
//! let p: Vec<f64> = escenario
//!     .candidatas
//!     .iter()
//!     .map(|c| prueba_por_bloques(&c[..1000], &escenario.etiqueta[..1000], 50, 199, &mut g, dos_colas))
//!     .map(|prueba| prueba.unwrap().p_valor)
//!     .collect();
//! let elegidas = benjamini_hochberg(&p, 0.10).unwrap();
//! assert!(elegidas[2]);
//!
//! // Confirmación: la reserva se abre una vez.
//! let (x, y): (Vec<f64>, Vec<f64>) = reserva.abrir().into_iter().unzip();
//! let confirmacion = prueba_por_bloques(&x, &y, 50, 199, &mut g, dos_colas).unwrap();
//! assert!(confirmacion.p_valor <= 0.05);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod azar;
pub mod fdr;
pub mod normal;
pub mod particion;
pub mod permutacion;
pub mod sellado;
mod sha256;
pub mod sintetico;
