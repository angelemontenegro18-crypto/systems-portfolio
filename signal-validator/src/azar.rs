//! Generador pseudoaleatorio pequeño y reproducible.
//!
//! Todo lo aleatorio del crate —los datos sintéticos, las permutaciones, los
//! pliegues barajados— sale de aquí, siempre con una semilla explícita. La misma
//! semilla da exactamente los mismos números en cualquier plataforma: el
//! generador solo usa aritmética entera, y la normal solo sumas.

/// Suma que avanza el estado en cada paso: la parte fraccionaria de la razón
/// áurea en 64 bits. Es impar, así que el contador recorre los 2⁶⁴ valores
/// antes de repetirse.
const INCREMENTO: u64 = 0x9E37_79B9_7F4A_7C15;

/// Generador SplitMix64.
///
/// Es la variante de incremento fijo del generador que describen Steele, Lea y
/// Flood (*Fast splittable pseudorandom number generators*, OOPSLA 2014). El
/// estado es un contador de 64 bits; cada salida es ese contador pasado por una
/// función de mezcla. Ninguna semilla es mala, ni siquiera el cero.
///
/// **No es criptográfico.** Sirve para simular y para permutar, no para claves.
///
/// ```
/// use signal_validator::azar::Generador;
///
/// let mut a = Generador::nuevo(7);
/// let mut b = Generador::nuevo(7);
/// assert_eq!(a.siguiente(), b.siguiente());
/// ```
#[derive(Debug, Clone)]
pub struct Generador {
    estado: u64,
}

impl Generador {
    /// Generador con la semilla dada.
    pub fn nuevo(semilla: u64) -> Generador {
        Generador { estado: semilla }
    }

    /// Siguiente número de 64 bits.
    pub fn siguiente(&mut self) -> u64 {
        self.estado = self.estado.wrapping_add(INCREMENTO);
        // Las dos multiplicaciones y los tres corrimientos son la función de
        // mezcla de la referencia; cambiar cualquiera rompe los vectores de prueba.
        let mut z = self.estado;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniforme en `[0, 1)`, con 53 bits de resolución: los 53 bits altos de
    /// [`siguiente`](Self::siguiente) escalados por 2⁻⁵³, que es exacto.
    pub fn uniforme(&mut self) -> f64 {
        const ESCALA: f64 = 1.0 / (1u64 << 53) as f64;
        (self.siguiente() >> 11) as f64 * ESCALA
    }

    /// Variable aproximadamente normal estándar: la suma de 12 uniformes menos 6.
    ///
    /// Media 0 y varianza exactamente 1 (cada uniforme aporta 1/12). No es una
    /// normal exacta —las colas terminan en ±6—, pero para datos sintéticos
    /// alcanza. Se prefirió a Box–Muller a propósito: usa solo sumas, así que
    /// da los mismos bits en cualquier plataforma, mientras que Box–Muller
    /// depende de `ln`, `cos` y `sin` de la biblioteca matemática del sistema,
    /// que pueden diferir en el último bit.
    pub fn normal(&mut self) -> f64 {
        // Lazo explícito, no `Iterator::sum`: el orden de las sumas fija el
        // resultado bit a bit.
        let mut suma = 0.0;
        for _ in 0..12 {
            suma += self.uniforme();
        }
        suma - 6.0
    }

    /// Entero uniforme en `[0, n)`, **sin sesgo**.
    ///
    /// `siguiente() % n` favorece a los restos chicos cuando `n` no divide a
    /// 2⁶⁴. Aquí se descartan los valores de la franja que sobra (los primeros
    /// `2⁶⁴ mod n`), así que cada resto sale exactamente la misma cantidad de
    /// veces.
    ///
    /// # Pánico
    ///
    /// Si `n` es cero.
    pub fn entero_bajo(&mut self, n: u64) -> u64 {
        assert!(n > 0, "entero_bajo: el rango [0, 0) está vacío");
        // 2⁶⁴ mod n, calculado sin salir de u64.
        let sobrante = n.wrapping_neg() % n;
        loop {
            let x = self.siguiente();
            if x >= sobrante {
                return x % n;
            }
        }
    }

    /// Baraja `valores` en el lugar, con Fisher–Yates: cada permutación sale
    /// con la misma probabilidad.
    pub fn barajar<T>(&mut self, valores: &mut [T]) {
        for i in (1..valores.len()).rev() {
            let j = self.entero_bajo(i as u64 + 1) as usize;
            valores.swap(i, j);
        }
    }
}
