//! Las órdenes al actuador.
//!
//! [`Orden`] está **sellado**: solo las órdenes de este módulo lo implementan, así que la
//! política sabe de antemano todo lo que puede llegarle. Un tipo de afuera no puede hacerse
//! pasar por una orden:
//!
//! ```compile_fail,E0277
//! use policy_kernel::orden::Orden;
//! use policy_kernel::regla::Clase;
//!
//! struct Inventada;
//!
//! // error[E0277]: falta el supertrait sellado, que no es accesible desde afuera.
//! impl Orden for Inventada {
//!     fn clase(&self) -> Clase {
//!         Clase::Apertura
//!     }
//!     fn valor(&self) -> i32 {
//!         50
//!     }
//! }
//! ```

use crate::regla::Clase;

mod sellado {
    /// El supertrait privado que sella a [`Orden`](super::Orden).
    pub trait Sellado {}
}

/// Una orden para el actuador: su clase y el valor que las reglas acotan.
pub trait Orden: sellado::Sellado {
    /// La clase de reglas a la que se somete.
    fn clase(&self) -> Clase;
    /// El valor que se compara con las reglas.
    fn valor(&self) -> i32;
}

/// Mover la válvula a una apertura, en %.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoverValvula {
    /// La apertura pedida, en %.
    pub apertura: u8,
}

/// Fijar la presión del circuito, en bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FijarPresion {
    /// La presión pedida, en bar.
    pub bar: u8,
}

/// Fijar la velocidad de avance del pistón, en mm/s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FijarAvance {
    /// La velocidad pedida, en mm/s.
    pub mm_por_s: u8,
}

/// Purgar el circuito durante unos segundos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purgar {
    /// La duración pedida, en segundos.
    pub segundos: u8,
}

impl sellado::Sellado for MoverValvula {}
impl sellado::Sellado for FijarPresion {}
impl sellado::Sellado for FijarAvance {}
impl sellado::Sellado for Purgar {}

impl Orden for MoverValvula {
    fn clase(&self) -> Clase {
        Clase::Apertura
    }
    fn valor(&self) -> i32 {
        i32::from(self.apertura)
    }
}

impl Orden for FijarPresion {
    fn clase(&self) -> Clase {
        Clase::Presion
    }
    fn valor(&self) -> i32 {
        i32::from(self.bar)
    }
}

impl Orden for FijarAvance {
    fn clase(&self) -> Clase {
        Clase::Avance
    }
    fn valor(&self) -> i32 {
        i32::from(self.mm_por_s)
    }
}

impl Orden for Purgar {
    fn clase(&self) -> Clase {
        Clase::Purga
    }
    fn valor(&self) -> i32 {
        i32::from(self.segundos)
    }
}
