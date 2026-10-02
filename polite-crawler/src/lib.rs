//! # polite-crawler
//!
//! Un crawler **honesto**: dice quién es, pide permiso y no apura a nadie.
//!
//! - Se presenta con una [identidad](Identidad) declarada — nombre, versión y
//!   un contacto — que no puede imitar a un navegador ni al bot de otro, y que
//!   no cambia durante su vida.
//! - Lee `robots.txt` según la [RFC 9309](robots), y **no hay forma de
//!   ignorarlo**: no existe ninguna opción que lo desactive.
//! - Respeta un intervalo mínimo por origen, el `Crawl-delay` del sitio, un
//!   presupuesto de páginas, `Retry-After`, y aplica backoff ante fallas.
//! - Solo hace `GET`, y no sigue redirecciones por su cuenta: cada salto pasa
//!   por las mismas reglas.
//!
//! ```no_run
//! use std::time::{Duration, Instant};
//! use polite_crawler::{Config, Identidad, Rastreador, Rechazo, TransporteHttp};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let identidad = Identidad::nueva("LectorDocs", "0.1", "https://ejemplo.test/bot")?;
//! let transporte = TransporteHttp::nuevo(Duration::from_secs(10), 1 << 20);
//! let mut rastreador = Rastreador::nuevo(transporte, identidad, Config::default());
//!
//! let inicio = Instant::now();
//! loop {
//!     let ahora = inicio.elapsed().as_millis() as u64;
//!     match rastreador.obtener("https://ejemplo.test/documentacion", ahora) {
//!         Ok(pagina) => { println!("{} bytes", pagina.cuerpo.len()); break; }
//!         Err(Rechazo::Esperar { desde_ms, .. }) => {
//!             std::thread::sleep(Duration::from_millis(desde_ms.saturating_sub(ahora)));
//!         }
//!         Err(otro) => { println!("no: {otro}"); break; }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod identidad;
pub mod rastreador;
pub mod robots;
pub mod transporte;

pub use identidad::{ErrorIdentidad, Identidad};
pub use rastreador::{Config, MotivoEspera, Pagina, Rastreador, Rechazo};
pub use robots::{Decision, Politica, Robots};
pub use transporte::{Respuesta, Transporte, TransporteHttp};
