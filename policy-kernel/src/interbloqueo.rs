//! El interbloqueo del actuador, con su estado en el tipo.
//!
//! Un `Interbloqueo<Desarmado>` no aplica nada: el método no existe. Para aplicar hay que
//! armarlo, y armarlo exige el resguardo cerrado; si no lo está, sigue desarmado. Armado,
//! [`aplicar`](Interbloqueo::aplicar) recibe una [`Autorizada`] —no una orden suelta— y
//! devuelve el [`Comando`] para el hardware, que tampoco se fabrica de otra forma.
//!
//! ```compile_fail,E0599
//! use policy_kernel::interbloqueo::Interbloqueo;
//! use policy_kernel::orden::MoverValvula;
//! use policy_kernel::politica::{Decision, PRENSA};
//!
//! let mut desarmado = Interbloqueo::nuevo();
//! if let Decision::Permitir(autorizada) = PRENSA.evaluar(MoverValvula { apertura: 50 }) {
//!     // error[E0599]: un interbloqueo desarmado no tiene `aplicar`.
//!     let _ = desarmado.aplicar(autorizada);
//! }
//! ```
//!
//! Y armado, tampoco acepta una orden que no pasó por la política:
//!
//! ```compile_fail,E0308
//! use policy_kernel::interbloqueo::Interbloqueo;
//! use policy_kernel::orden::MoverValvula;
//!
//! let Ok(mut armado) = Interbloqueo::nuevo().armar(true) else { return };
//! // error[E0308]: se esperaba una `Autorizada`, no una orden sin evaluar.
//! let _ = armado.aplicar(MoverValvula { apertura: 50 });
//! ```

use core::marker::PhantomData;

use crate::orden::Orden;
use crate::politica::Autorizada;
use crate::regla::Clase;

/// El estado de un interbloqueo que no aplica nada.
#[derive(Debug)]
pub enum Desarmado {}

/// El estado de un interbloqueo que aplica órdenes autorizadas.
#[derive(Debug)]
pub enum Armado {}

/// El interbloqueo del actuador, en el estado `Estado`.
#[derive(Debug)]
pub struct Interbloqueo<Estado> {
    aplicadas: u32,
    estado: PhantomData<Estado>,
}

/// Lo que va al hardware: la clase y el valor de una orden autorizada y aplicada con el
/// interbloqueo armado. Solo lo produce [`Interbloqueo::aplicar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comando {
    clase: Clase,
    valor: i32,
}

impl Comando {
    /// La clase de la orden.
    pub fn clase(&self) -> Clase {
        self.clase
    }

    /// El valor de la orden.
    pub fn valor(&self) -> i32 {
        self.valor
    }
}

impl Interbloqueo<Desarmado> {
    /// Un interbloqueo nuevo: desarmado.
    pub const fn nuevo() -> Interbloqueo<Desarmado> {
        Interbloqueo {
            aplicadas: 0,
            estado: PhantomData,
        }
    }

    /// Lo arma si el resguardo está cerrado. Si no, lo devuelve desarmado.
    pub fn armar(
        self,
        resguardo_cerrado: bool,
    ) -> Result<Interbloqueo<Armado>, Interbloqueo<Desarmado>> {
        if resguardo_cerrado {
            Ok(Interbloqueo {
                aplicadas: self.aplicadas,
                estado: PhantomData,
            })
        } else {
            Err(self)
        }
    }
}

impl Interbloqueo<Armado> {
    /// Aplica una orden autorizada, consumiendo la autorización, y devuelve el comando para
    /// el hardware.
    pub fn aplicar<O: Orden>(&mut self, autorizada: Autorizada<'_, O>) -> Comando {
        self.aplicadas = self.aplicadas.saturating_add(1);
        let orden = autorizada.orden();
        Comando {
            clase: orden.clase(),
            valor: orden.valor(),
        }
    }

    /// Lo desarma.
    pub fn desarmar(self) -> Interbloqueo<Desarmado> {
        Interbloqueo {
            aplicadas: self.aplicadas,
            estado: PhantomData,
        }
    }
}

impl<Estado> Interbloqueo<Estado> {
    /// Cuántas órdenes aplicó, en toda su vida.
    pub fn aplicadas(&self) -> u32 {
        self.aplicadas
    }
}
