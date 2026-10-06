//! Hamming ampliado (8,4): cuatro bits de dato en una palabra de ocho, con distancia mínima
//! cuatro. Corrige cualquier error de un bit y detecta cualquier error de dos.
//!
//! Disposición de la palabra (el bit 0 es el menos significativo):
//!
//! | bit       | 0              | 1    | 2    | 3    | 4    | 5    | 6    | 7    |
//! |-----------|----------------|------|------|------|------|------|------|------|
//! | contenido | paridad global | `p1` | `p2` | `d0` | `p4` | `d1` | `d2` | `d3` |
//!
//! Los bits 1 a 7 son un Hamming (7,4) clásico: la paridad `pₖ` cubre las posiciones cuyo
//! índice tiene encendido el bit de valor `k`, así que el **síndrome** —el XOR de las
//! posiciones de los bits encendidos— vale cero en una palabra válida y, con un error, señala
//! su posición. La **paridad global** (bit 0) es la que separa un error (paridad impar) de
//! dos (paridad par con síndrome distinto de cero).
//!
//! Con tres o más errores no hay garantía: la palabra puede decodificarse a un dato
//! equivocado.

use crate::Error;

/// El resultado de decodificar una palabra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decodificacion {
    /// La palabra llegó sin errores. Lleva el dato.
    Intacta(u8),
    /// Había un bit con error, y se corrigió. Lleva el dato.
    Corregida(u8),
    /// Hay dos bits con error: se sabe que el dato está mal, pero no cuál era.
    DobleError,
}

/// Codifica un dato de 4 bits (0 a 15). Si es mayor, [`Error::FueraDeRango`].
pub fn codificar(dato: u8) -> Result<u8, Error> {
    if dato > 0x0F {
        return Err(Error::FueraDeRango);
    }
    Ok(palabra_de(dato))
}

/// Decodificación dura: corrige un error, detecta dos.
pub fn decodificar(palabra: u8) -> Decodificacion {
    let sindrome = (1..8u8)
        .filter(|&posicion| (palabra >> posicion) & 1 == 1)
        .fold(0u8, |s, posicion| s ^ posicion);
    let paridad_impar = palabra.count_ones() % 2 == 1;
    match (sindrome, paridad_impar) {
        (0, false) => Decodificacion::Intacta(dato_de(palabra)),
        // El síndrome es cero y la paridad no cierra: el error está en la paridad global,
        // que no lleva dato.
        (0, true) => Decodificacion::Corregida(dato_de(palabra)),
        (posicion, true) => Decodificacion::Corregida(dato_de(palabra ^ (1 << posicion))),
        (_, false) => Decodificacion::DobleError,
    }
}

/// Decodificación blanda: de las 16 palabras válidas, la que más se parece a `blandos`.
/// Devuelve su dato.
///
/// `blandos[j]` es la correlación del bit `j` de la palabra, como la entrega
/// [`correlar`](crate::ensanchado::correlar): positiva si parece un 0, negativa si parece un
/// 1, y mayor en magnitud cuanto más clara. La métrica de cada palabra válida es
/// `Σ blandos[j] · (+1 si su bit j es 0, −1 si es 1)`; gana la mayor y, en empate, el dato
/// menor. A diferencia de la decisión dura, aprovecha que un bit dudoso pesa poco.
///
/// La métrica se suma en `i128`, exacta para cualquier entrada: ocho valores de 64 bits no
/// pueden desbordarla.
pub fn decodificar_blando(blandos: &[i64; 8]) -> u8 {
    let mut mejor_dato = 0u8;
    let mut mejor_metrica = i128::MIN;
    for dato in 0..16u8 {
        let palabra = palabra_de(dato);
        let metrica: i128 = blandos
            .iter()
            .enumerate()
            .map(|(j, &v)| {
                let v = i128::from(v);
                if (palabra >> j) & 1 == 0 {
                    v
                } else {
                    -v
                }
            })
            .sum();
        if metrica > mejor_metrica {
            mejor_dato = dato;
            mejor_metrica = metrica;
        }
    }
    mejor_dato
}

/// La palabra de los 4 bits bajos de `dato`.
const fn palabra_de(dato: u8) -> u8 {
    let d0 = dato & 1;
    let d1 = (dato >> 1) & 1;
    let d2 = (dato >> 2) & 1;
    let d3 = (dato >> 3) & 1;
    let p1 = d0 ^ d1 ^ d3; // posiciones 3, 5, 7
    let p2 = d0 ^ d2 ^ d3; // posiciones 3, 6, 7
    let p4 = d1 ^ d2 ^ d3; // posiciones 5, 6, 7
    let siete = (p1 << 1) | (p2 << 2) | (d0 << 3) | (p4 << 4) | (d1 << 5) | (d2 << 6) | (d3 << 7);
    siete | (siete.count_ones() & 1) as u8
}

/// Los 4 bits de dato de una palabra (posiciones 3, 5, 6 y 7).
const fn dato_de(palabra: u8) -> u8 {
    ((palabra >> 3) & 1)
        | (((palabra >> 5) & 1) << 1)
        | (((palabra >> 6) & 1) << 2)
        | (((palabra >> 7) & 1) << 3)
}
