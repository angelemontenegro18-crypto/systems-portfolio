//! Códigos Gold: una familia de `N + 2` códigos de largo `N = 2ⁿ − 1` cuya correlación
//! cruzada, para cualquier par y cualquier desfase, toma solo tres valores.
//!
//! Se arman con dos secuencias de máximo largo `a` y `b` de un **par preferente**: la familia
//! es `a`, `b`, y `a · (b desplazada k)` para cada `k` de `0` a `N − 1`. Si `n` es impar, los
//! tres valores son `−1`, `−t` y `t − 2`, con `t = 1 + 2^((n+1)/2)`; si `n ≡ 2 (mod 4)`,
//! `t = 1 + 2^((n+2)/2)`. Es lo que permite que varios sensores compartan el canal con
//! códigos distintos sin que uno ahogue a otro.
//!
//! Los pares de la tabla son los preferentes estándar de grado 5, 6 y 7, y no hay otros: la
//! cota de tres valores es una propiedad del par, no de dos secuencias cualesquiera, así que
//! [`ParPreferente`] no se puede armar desde fuera.
//!
//! ```compile_fail,E0451
//! use spread_spectrum_codec::gold::ParPreferente;
//! use spread_spectrum_codec::lfsr::Polinomio;
//!
//! // error[E0451]: los campos son privados; solo valen los pares de la tabla.
//! let cualquiera = ParPreferente { a: Polinomio::GRADO_5, b: Polinomio::GRADO_5 };
//! ```

use crate::lfsr::{Codigo, Polinomio};

/// Un par preferente de polinomios del mismo grado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParPreferente {
    a: Polinomio,
    b: Polinomio,
}

impl ParPreferente {
    /// Largo 31: `x⁵ + x² + 1` y `x⁵ + x⁴ + x³ + x² + 1`.
    pub const DE_31: ParPreferente = ParPreferente {
        a: Polinomio::GRADO_5,
        b: Polinomio::GRADO_5_B,
    };
    /// Largo 63: `x⁶ + x + 1` y `x⁶ + x⁵ + x² + x + 1`.
    pub const DE_63: ParPreferente = ParPreferente {
        a: Polinomio::GRADO_6,
        b: Polinomio::GRADO_6_B,
    };
    /// Largo 127: `x⁷ + x³ + 1` y `x⁷ + x³ + x² + x + 1`.
    pub const DE_127: ParPreferente = ParPreferente {
        a: Polinomio::GRADO_7,
        b: Polinomio::GRADO_7_B,
    };

    /// El grado `n` de los dos polinomios.
    pub const fn grado(&self) -> u8 {
        self.a.grado()
    }

    /// El largo de cada código, `N = 2ⁿ − 1`.
    pub const fn largo(&self) -> usize {
        self.a.periodo()
    }

    /// Cuántos códigos tiene la familia: `N + 2`.
    pub const fn tamano_de_familia(&self) -> usize {
        self.largo() + 2
    }

    /// El código `indice` de la familia: el 0 es la secuencia de `a`, el 1 la de `b`, y el
    /// `2 + k` es `a · (b desplazada k)`. Fuera de rango, `None`.
    pub fn codigo(&self, indice: usize) -> Option<Codigo> {
        let a = Codigo::secuencia_m(self.a);
        let b = Codigo::secuencia_m(self.b);
        match indice {
            0 => Some(a),
            1 => Some(b),
            i if i < self.tamano_de_familia() => a.producto_desplazado(&b, i - 2).ok(),
            _ => None,
        }
    }

    /// Los tres valores que puede tomar la correlación cruzada: `[−1, −t, t − 2]`.
    pub const fn valores_de_correlacion(&self) -> [i32; 3] {
        let n = self.grado() as u32;
        // (n + 1) / 2 si n es impar; (n + 2) / 2 si n ≡ 2 (mod 4).
        let exponente = if n % 2 == 1 { n.div_ceil(2) } else { n / 2 + 1 };
        let t = 1 + (1i32 << exponente);
        [-1, -t, t - 2]
    }
}
