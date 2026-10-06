//! # policy-kernel
//!
//! El núcleo de política del interbloqueo de seguridad de un actuador —la válvula y el
//! pistón de una prensa—, en `no_std`, sin `alloc` y sin dependencias. Las garantías se
//! mueven del momento de ejecutar al de compilar:
//!
//! - [`regla`]: la tabla de reglas se **valida al compilar** (ordenada, sin solapamientos, sin
//!   rangos vacíos), y un [`Rango`](regla::Rango) vacío es un error de tipo.
//! - [`orden`]: las órdenes al actuador, con el trait [`Orden`](orden::Orden) **sellado**.
//! - [`politica`]: la evaluación, que **cierra en falla**, y la [`Autorizada`](politica::Autorizada),
//!   la capacidad de aplicar una orden, que solo la política entrega.
//! - [`interbloqueo`]: el interbloqueo con su estado en el tipo; **solo armado** aplica, y solo
//!   órdenes autorizadas.
//!
//! ```
//! use policy_kernel::interbloqueo::Interbloqueo;
//! use policy_kernel::orden::MoverValvula;
//! use policy_kernel::politica::{Decision, Motivo, PRENSA};
//! use policy_kernel::regla::Clase;
//!
//! let Ok(mut interbloqueo) = Interbloqueo::nuevo().armar(true) else {
//!     unreachable!("el resguardo está cerrado")
//! };
//!
//! // 50 % está permitido: la política da la autorización, y el interbloqueo la aplica.
//! match PRENSA.evaluar(MoverValvula { apertura: 50 }) {
//!     Decision::Permitir(autorizada) => {
//!         let comando = interbloqueo.aplicar(autorizada);
//!         assert_eq!((comando.clase(), comando.valor()), (Clase::Apertura, 50));
//!     }
//!     Decision::Denegar(_) => unreachable!(),
//! }
//!
//! // 85 % cae en la banda donde la válvula vibra: denegado, y no hay nada que aplicar.
//! assert!(matches!(
//!     PRENSA.evaluar(MoverValvula { apertura: 85 }),
//!     Decision::Denegar(Motivo::FueraDeRango)
//! ));
//! assert_eq!(interbloqueo.aplicadas(), 1);
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

pub mod interbloqueo;
pub mod orden;
pub mod politica;
pub mod regla;
