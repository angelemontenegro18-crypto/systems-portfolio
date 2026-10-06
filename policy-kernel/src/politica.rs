//! La política: evalúa órdenes contra una tabla válida y entrega autorizaciones.
//!
//! [`Politica::evaluar`] **cierra en falla**: la decisión empieza en `Denegar` y solo cambia
//! si una regla de la clase contiene el valor. Lo único que sale de un `Permitir` es una
//! [`Autorizada`], la capacidad de aplicar esa orden: tiene campos privados y nadie más la
//! fabrica.
//!
//! ```compile_fail,E0451
//! use core::marker::PhantomData;
//! use policy_kernel::orden::MoverValvula;
//! use policy_kernel::politica::Autorizada;
//!
//! // error[E0451]: una autorización no se fabrica; se la pide a la política.
//! let falsa = Autorizada { orden: MoverValvula { apertura: 50 }, politica: PhantomData };
//! ```
//!
//! La autorización toma prestada a la política que la dio: mientras viva, esa política no se
//! puede cambiar ni soltar.

use core::marker::PhantomData;

use crate::orden::Orden;
use crate::regla::{validar, Clase, Rango, Regla};

/// Una tabla de reglas que pasó [`validar`]. Solo se construye desde una tabla válida, así que
/// evaluar siempre trabaja sobre una tabla ordenada y sin solapamientos.
#[derive(Debug, Clone, Copy)]
pub struct Politica<'r> {
    reglas: &'r [Regla],
}

/// Por qué se denegó una orden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    /// No hay ninguna regla para la clase de la orden.
    SinRegla,
    /// Hay reglas para la clase, pero ninguna contiene el valor.
    FueraDeRango,
}

/// El resultado de evaluar una orden.
#[derive(Debug)]
#[must_use = "una orden evaluada que no se mira no se aplica"]
pub enum Decision<'p, O: Orden> {
    /// La orden está permitida: aquí está la capacidad de aplicarla.
    Permitir(Autorizada<'p, O>),
    /// La orden está denegada, y por qué.
    Denegar(Motivo),
}

/// La capacidad de aplicar una orden: solo la entrega [`Politica::evaluar`], y aplicarla la
/// consume.
#[derive(Debug)]
#[must_use = "una autorización que no se aplica no hace nada"]
pub struct Autorizada<'p, O: Orden> {
    orden: O,
    politica: PhantomData<&'p ()>,
}

impl<O: Orden> Autorizada<'_, O> {
    /// La orden autorizada.
    pub fn orden(&self) -> &O {
        &self.orden
    }
}

impl<'r> Politica<'r> {
    /// Una política sobre `reglas`, si la tabla es válida; si no, `None`. Es `const`: con la
    /// tabla en un `const` y `assert!(validar(…))`, una tabla inválida no compila.
    pub const fn con_reglas(reglas: &'r [Regla]) -> Option<Politica<'r>> {
        if validar(reglas) {
            Some(Politica { reglas })
        } else {
            None
        }
    }

    /// Las reglas.
    pub const fn reglas(&self) -> &'r [Regla] {
        self.reglas
    }

    /// Evalúa `orden`. Recorre las reglas de su clase en orden de mínimo y corta en cuanto
    /// una empieza después del valor: como la tabla está ordenada, ninguna de las que siguen
    /// puede contenerlo.
    pub fn evaluar<O: Orden>(&self, orden: O) -> Decision<'_, O> {
        let (clase, valor) = (orden.clase(), orden.valor());
        let mut motivo = Motivo::SinRegla;
        for regla in self.reglas.iter().filter(|r| r.clase() == clase) {
            motivo = Motivo::FueraDeRango;
            if valor < regla.min() {
                break;
            }
            if regla.contiene(valor) {
                return Decision::Permitir(Autorizada {
                    orden,
                    politica: PhantomData,
                });
            }
        }
        Decision::Denegar(motivo)
    }
}

/// Las reglas del interbloqueo de la prensa.
///
/// - **Apertura:** de 0 a 80 % y de 90 a 100 %. Entre 81 y 89 % la válvula vibra; por encima
///   de 100 no hay apertura.
/// - **Presión:** de 0 a 160 bar.
/// - **Avance:** de 0 a 25 mm/s.
/// - **Purga:** ninguna regla. Purgar con la prensa en marcha no se permite nunca.
pub const REGLAS_DE_LA_PRENSA: [Regla; 4] = [
    Rango::<0, 80>.regla(Clase::Apertura),
    Rango::<90, 100>.regla(Clase::Apertura),
    Rango::<0, 160>.regla(Clase::Presion),
    Rango::<0, 25>.regla(Clase::Avance),
];

const _: () = assert!(
    validar(&REGLAS_DE_LA_PRENSA),
    "la tabla de la prensa no es válida"
);

/// La política de la prensa, sobre [`REGLAS_DE_LA_PRENSA`].
pub const PRENSA: Politica<'static> = Politica {
    reglas: &REGLAS_DE_LA_PRENSA,
};
