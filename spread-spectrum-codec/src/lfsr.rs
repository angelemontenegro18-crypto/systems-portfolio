//! Secuencias de máximo largo, generadas con un registro de desplazamiento con
//! realimentación lineal (LFSR).
//!
//! Con un polinomio **primitivo** de grado `n`, el registro recorre los `2ⁿ − 1` estados
//! distintos de cero antes de repetirse. La secuencia que sale tiene tres propiedades que la
//! hacen útil para esparcir:
//!
//! - **Periodo** `N = 2ⁿ − 1`.
//! - **Equilibrio**: en un periodo hay un 1 más que ceros.
//! - **Autocorrelación de dos valores**: en chips ±1, la correlación periódica de la secuencia
//!   consigo misma vale `N` sin desfase y `−1` con cualquier otro desfase.
//!
//! Los polinomios son los primitivos de las tablas estándar, y son los únicos que existen:
//! [`Polinomio`] no se puede construir fuera de este módulo, así que no hay forma de pasarle al
//! registro uno sin verificar.
//!
//! ```compile_fail,E0451
//! use spread_spectrum_codec::lfsr::Polinomio;
//!
//! // error[E0451]: los campos son privados; solo valen los polinomios de la tabla.
//! let inventado = Polinomio { grado: 5, coeficientes: 0b10_0111 };
//! ```

use crate::Error;

/// El largo máximo de un código: grado 7, `2⁷ − 1` chips.
pub const LARGO_MAXIMO: usize = 127;

/// Un polinomio de realimentación `xⁿ + … + 1`, con coeficientes en GF(2).
///
/// El bit `k` de los coeficientes es el coeficiente de `xᵏ`. Solo existen los de la tabla,
/// todos primitivos: lo comprueba la suite de pruebas midiendo el periodo de cada uno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Polinomio {
    grado: u8,
    coeficientes: u16,
}

impl Polinomio {
    /// `x³ + x + 1`: periodo 7.
    pub const GRADO_3: Polinomio = Polinomio {
        grado: 3,
        coeficientes: 0b1011,
    };
    /// `x⁴ + x + 1`: periodo 15.
    pub const GRADO_4: Polinomio = Polinomio {
        grado: 4,
        coeficientes: 0b1_0011,
    };
    /// `x⁵ + x² + 1`: periodo 31.
    pub const GRADO_5: Polinomio = Polinomio {
        grado: 5,
        coeficientes: 0b10_0101,
    };
    /// `x⁶ + x + 1`: periodo 63.
    pub const GRADO_6: Polinomio = Polinomio {
        grado: 6,
        coeficientes: 0b100_0011,
    };
    /// `x⁷ + x³ + 1`: periodo 127.
    pub const GRADO_7: Polinomio = Polinomio {
        grado: 7,
        coeficientes: 0b1000_1001,
    };
    /// `x⁵ + x⁴ + x³ + x² + 1`: el compañero de [`Polinomio::GRADO_5`] en el par preferente
    /// de largo 31.
    pub const GRADO_5_B: Polinomio = Polinomio {
        grado: 5,
        coeficientes: 0b11_1101,
    };
    /// `x⁶ + x⁵ + x² + x + 1`: el compañero de [`Polinomio::GRADO_6`] en el par preferente
    /// de largo 63.
    pub const GRADO_6_B: Polinomio = Polinomio {
        grado: 6,
        coeficientes: 0b110_0111,
    };
    /// `x⁷ + x³ + x² + x + 1`: el compañero de [`Polinomio::GRADO_7`] en el par preferente
    /// de largo 127.
    pub const GRADO_7_B: Polinomio = Polinomio {
        grado: 7,
        coeficientes: 0b1000_1111,
    };

    /// Uno por grado, de 3 a 7.
    pub const POR_GRADO: [Polinomio; 5] = [
        Polinomio::GRADO_3,
        Polinomio::GRADO_4,
        Polinomio::GRADO_5,
        Polinomio::GRADO_6,
        Polinomio::GRADO_7,
    ];

    /// Todos los de la tabla: uno por grado y los tres compañeros de los pares preferentes.
    pub const TODOS: [Polinomio; 8] = [
        Polinomio::GRADO_3,
        Polinomio::GRADO_4,
        Polinomio::GRADO_5,
        Polinomio::GRADO_6,
        Polinomio::GRADO_7,
        Polinomio::GRADO_5_B,
        Polinomio::GRADO_6_B,
        Polinomio::GRADO_7_B,
    ];

    /// El grado `n`.
    pub const fn grado(&self) -> u8 {
        self.grado
    }

    /// El periodo de la secuencia, `2ⁿ − 1`.
    pub const fn periodo(&self) -> usize {
        (1usize << self.grado) - 1
    }
}

/// El registro de desplazamiento, en su forma de Fibonacci.
///
/// El estado guarda los últimos `n` bits de la secuencia; en cada paso sale el más viejo y
/// entra la suma módulo 2 de los que marca el polinomio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lfsr {
    estado: u16,
    mascara: u16,
    grado: u8,
}

impl Lfsr {
    /// Un registro que arranca con todos sus bits en 1 (cualquier estado distinto de cero
    /// recorre el mismo ciclo).
    pub const fn nuevo(polinomio: Polinomio) -> Lfsr {
        let todos_unos = (1u16 << polinomio.grado) - 1;
        Lfsr {
            estado: todos_unos,
            mascara: polinomio.coeficientes & todos_unos,
            grado: polinomio.grado,
        }
    }

    /// El estado actual: los `n` bits más bajos.
    pub const fn estado(&self) -> u16 {
        self.estado
    }

    /// Avanza un paso y devuelve el bit que sale (0 o 1).
    pub fn siguiente(&mut self) -> u8 {
        let salida = (self.estado & 1) as u8;
        let entra = ((self.estado & self.mascara).count_ones() & 1) as u16;
        self.estado = (self.estado >> 1) | (entra << (self.grado - 1));
        salida
    }
}

/// El chip de un bit, en modulación de fase binaria: el 0 es `+1` y el 1 es `−1`.
pub const fn chip_de(bit: u8) -> i8 {
    if bit == 0 {
        1
    } else {
        -1
    }
}

/// Una secuencia de chips (`+1` o `−1`) de largo fijo, sin `alloc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codigo {
    chips: [i8; LARGO_MAXIMO],
    largo: usize,
}

impl Codigo {
    /// La secuencia de máximo largo del polinomio: un periodo completo.
    pub fn secuencia_m(polinomio: Polinomio) -> Codigo {
        let mut registro = Lfsr::nuevo(polinomio);
        let largo = polinomio.periodo().min(LARGO_MAXIMO);
        let mut chips = [0i8; LARGO_MAXIMO];
        for chip in chips.iter_mut().take(largo) {
            *chip = chip_de(registro.siguiente());
        }
        Codigo { chips, largo }
    }

    /// Los chips.
    pub fn chips(&self) -> &[i8] {
        self.chips.get(..self.largo).unwrap_or(&[])
    }

    /// El largo: el factor de esparcido `N`.
    pub fn largo(&self) -> usize {
        self.largo
    }

    /// El producto chip a chip con `otro` desplazado `desfase` posiciones (en forma cíclica):
    /// la operación que arma los códigos Gold.
    pub fn producto_desplazado(&self, otro: &Codigo, desfase: usize) -> Result<Codigo, Error> {
        let n = self.largo;
        if n != otro.largo {
            return Err(Error::LargosDistintos);
        }
        let mut chips = [0i8; LARGO_MAXIMO];
        if n > 0 {
            let d = desfase % n;
            let b = otro.chips();
            for (k, (salida, &a)) in chips.iter_mut().zip(self.chips()).enumerate() {
                let y = b.get((k + d) % n).copied().unwrap_or(1);
                *salida = a * y;
            }
        }
        Ok(Codigo { chips, largo: n })
    }
}

/// La correlación periódica de dos códigos del mismo largo, con un desfase cíclico:
/// `Σ a[k] · b[(k + desfase) mod N]`.
pub fn correlacion_periodica(a: &Codigo, b: &Codigo, desfase: usize) -> Result<i32, Error> {
    let n = a.largo();
    if n != b.largo() {
        return Err(Error::LargosDistintos);
    }
    if n == 0 {
        return Ok(0);
    }
    let d = desfase % n;
    let bc = b.chips();
    let mut suma = 0i32;
    for (k, &x) in a.chips().iter().enumerate() {
        let y = bc.get((k + d) % n).copied().unwrap_or(0);
        suma += i32::from(x) * i32::from(y);
    }
    Ok(suma)
}
