//! Las reglas, y su validación en compilación.
//!
//! Una [`Regla`] permite los valores de un rango cerrado `[min, max]` para una [`Clase`] de
//! orden. Una tabla es válida si está **ordenada** por clase y, dentro de cada clase, por su
//! mínimo; si **no se solapa** (dos reglas de la misma clase no comparten ningún valor); y si
//! **ningún rango está vacío**. [`validar`] lo comprueba y es `const`, así que la tabla se
//! valida al compilar:
//!
//! ```
//! use policy_kernel::regla::{validar, Clase, Rango, Regla};
//!
//! const REGLAS: [Regla; 2] = [
//!     Rango::<0, 80>.regla(Clase::Apertura),
//!     Rango::<90, 100>.regla(Clase::Apertura),
//! ];
//! const _: () = assert!(validar(&REGLAS));
//! ```
//!
//! Una tabla con dos reglas solapadas no compila:
//!
//! ```compile_fail,E0080
//! use policy_kernel::regla::{validar, Clase, Rango, Regla};
//!
//! const REGLAS: [Regla; 2] = [
//!     Rango::<0, 80>.regla(Clase::Apertura),
//!     Rango::<70, 100>.regla(Clase::Apertura), // comparte 70..=80 con la anterior
//! ];
//! const _: () = assert!(validar(&REGLAS)); // error[E0080]
//! ```
//!
//! Y un [`Rango`] vacío tampoco: la aserción `MIN <= MAX` es parte del tipo.
//!
//! ```compile_fail,E0080
//! use policy_kernel::regla::{Clase, Rango, Regla};
//!
//! const VACIA: Regla = Rango::<100, 90>.regla(Clase::Apertura); // error[E0080]
//! # let _ = VACIA;
//! ```

/// A qué familia de reglas se somete una orden. El orden de las variantes es el orden de la
/// tabla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Clase {
    /// La apertura de la válvula, en %.
    Apertura,
    /// La presión del circuito, en bar.
    Presion,
    /// La velocidad de avance del pistón, en mm/s.
    Avance,
    /// Una purga, en segundos.
    Purga,
}

impl Clase {
    /// Todas, en el orden de la tabla.
    pub const TODAS: [Clase; 4] = [Clase::Apertura, Clase::Presion, Clase::Avance, Clase::Purga];
}

/// Permite los valores de `[min, max]` para las órdenes de una clase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regla {
    clase: Clase,
    min: i32,
    max: i32,
}

impl Regla {
    /// Una regla con los límites que se le den, sin comprobar nada: una tabla con una regla
    /// vacía (`min > max`) no pasa [`validar`]. Para fijar los límites en el tipo y que un
    /// rango vacío no compile, [`Rango::regla`].
    pub const fn nueva(clase: Clase, min: i32, max: i32) -> Regla {
        Regla { clase, min, max }
    }

    /// La clase.
    pub const fn clase(&self) -> Clase {
        self.clase
    }

    /// El mínimo permitido.
    pub const fn min(&self) -> i32 {
        self.min
    }

    /// El máximo permitido.
    pub const fn max(&self) -> i32 {
        self.max
    }

    /// Si `valor` está en `[min, max]`, extremos incluidos.
    pub const fn contiene(&self, valor: i32) -> bool {
        self.min <= valor && valor <= self.max
    }
}

/// Un rango cerrado `[MIN, MAX]` fijado en el tipo. La aserción `MIN <= MAX` se evalúa al
/// compilar: un rango vacío es un error de compilación, no de ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rango<const MIN: i32, const MAX: i32>;

impl<const MIN: i32, const MAX: i32> Rango<MIN, MAX> {
    /// La regla de este rango para `clase`.
    pub const fn regla(self, clase: Clase) -> Regla {
        const { assert!(MIN <= MAX, "rango vacío: MIN es mayor que MAX") };
        Regla {
            clase,
            min: MIN,
            max: MAX,
        }
    }
}

/// Si la tabla es válida: ordenada por clase y por mínimo, sin solapamientos dentro de una
/// clase, y sin rangos vacíos. Es `const`, para usarla en una aserción de compilación.
pub const fn validar(reglas: &[Regla]) -> bool {
    let mut resto = reglas;
    let mut anterior: Option<Regla> = None;
    while let [regla, cola @ ..] = resto {
        if regla.min > regla.max {
            return false;
        }
        if let Some(previa) = anterior {
            let (clase_previa, clase) = (previa.clase as u8, regla.clase as u8);
            if clase < clase_previa {
                return false;
            }
            // Misma clase: tiene que empezar después de donde terminó la anterior. Esto
            // rechaza a la vez el solapamiento y el desorden.
            if clase == clase_previa && regla.min <= previa.max {
                return false;
            }
        }
        anterior = Some(*regla);
        resto = cola;
    }
    true
}
