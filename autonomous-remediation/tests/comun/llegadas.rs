//! Llegadas sintéticas, para los tests y la demo.
//!
//! El generador es SplitMix64. El tiempo entre llegadas, en milisegundos, es
//! geométrico: en cada milisegundo llega una solicitud con probabilidad
//! `1/media`. Solo hay aritmética entera, divisiones y comparaciones: la misma
//! semilla da los mismos tiempos en cualquier plataforma.

/// Generador SplitMix64.
pub struct Generador(u64);

impl Generador {
    pub fn nuevo(semilla: u64) -> Generador {
        Generador(semilla)
    }

    pub fn siguiente(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniforme en `[0, 1)`, con 53 bits.
    pub fn uniforme(&mut self) -> f64 {
        (self.siguiente() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Milisegundos hasta la próxima llegada, con media `media`.
    pub fn intervalo(&mut self, media: f64) -> u64 {
        let p = 1.0 / media;
        let mut n = 1;
        while self.uniforme() >= p {
            n += 1;
        }
        n
    }
}

/// Una serie hecha de tramos `(cantidad, media)`, con una semilla.
pub fn tramos(semilla: u64, tramos: &[(usize, f64)]) -> Vec<u64> {
    let mut g = Generador::nuevo(semilla);
    let mut serie = Vec::new();
    for &(cantidad, media) in tramos {
        for _ in 0..cantidad {
            serie.push(g.intervalo(media));
        }
    }
    serie
}
