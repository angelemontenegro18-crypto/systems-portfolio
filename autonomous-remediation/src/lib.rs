//! # autonomous-remediation
//!
//! Decidir entre **autoaplicar** una remediación o **abrir un ticket** para una
//! persona, con cuatro puertas que tienen que pasar todas:
//!
//! 1. **autoridad** — la acción cae dentro de una [`Envolvente`] fijada por una
//!    persona, que la remediación no puede ampliar;
//! 2. **reversibilidad** — hay una vuelta atrás, y se arma y se verifica
//!    **antes** de aplicar;
//! 3. **beneficio neto acotado** — el [modelo de costos](costos) da beneficio
//!    mayor que cero y daño colateral no mayor que un tope;
//! 4. **confianza** — la señal alcanza un mínimo.
//!
//! Un dato ausente cierra la puerta que lo necesita. Si aplicar o la
//! comprobación posterior fallan, se revierte. Todo lo que no se autoaplica
//! termina en un [`Ticket`] que dice qué se iba a hacer, por qué, qué puerta
//! falló y con qué valores, y cómo revertir.
//!
//! Incluye un [detector] pasivo de cambios en el ritmo de llegadas (CUSUM),
//! que puede servir de señal.
//!
//! ```
//! use autonomous_remediation::{
//!     Accion, Desenlace, Envolvente, Metricas, Permiso, Politica, Propuesta, Servicio, Umbrales,
//! };
//!
//! let api = Servicio::nuevo("api").unwrap();
//! let envolvente = Envolvente::vacia().con(Permiso::escalar(api.clone(), 2, 6).unwrap());
//! let politica = Politica::nueva(envolvente, Umbrales::nuevos(0.9, 0.3).unwrap());
//!
//! let propuesta = Propuesta {
//!     accion: Accion::EscalarReplicas { servicio: api, actuales: 3, nuevas: 8 },
//!     motivo: "utilización 1.4 sostenida".to_string(),
//!     confianza: Some(0.97),
//!     metricas: Some(Metricas { tasa_de_error: 0.0, utilizacion: 1.4, aciertos_de_cache: 0.0, replicas: 3 }),
//! };
//! let evaluacion = politica.evaluar(&propuesta);
//! assert!(!evaluacion.autoaplicable()); // 8 réplicas está fuera de [2, 6]
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod accion;
pub mod costos;
pub mod detector;
mod envolvente;
mod politica;
mod remediador;
mod ticket;

pub use accion::{Accion, Servicio, TipoDeAccion};
pub use costos::{Metricas, Simulacion};
pub use envolvente::{Envolvente, Permiso};
pub use politica::{Evaluacion, Politica, Propuesta, Puerta, Umbrales, Veredicto};
pub use remediador::{Actuador, Desenlace, Remediador};
pub use ticket::{Estado, Ticket};
