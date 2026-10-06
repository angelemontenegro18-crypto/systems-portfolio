//! Un entrelazador de bloques: convierte una ráfaga de errores en errores aislados.
//!
//! Un bloque de `D` palabras de 8 bits se transmite por bits, primero el bit 0 de todas las
//! palabras, después el bit 1 de todas, y así: el bit `j` de la palabra `r` queda en la
//! posición `j · D + r`. Dos bits consecutivos de la trama son siempre de palabras distintas,
//! y `D` bits consecutivos cualesquiera son de `D` palabras distintas. Así, **una ráfaga de
//! hasta `D` bits toca cada palabra a lo sumo una vez**, y el Hamming (8,4) la corrige
//! entera. Una ráfaga de `D + 1` bits ya puede tocar dos veces la misma palabra.

use crate::Error;

/// Bits por palabra.
const BITS: usize = 8;

/// Entrelaza `palabras` (la profundidad `D` es `palabras.len()`) y escribe `8 · D` bits, cada
/// uno 0 o 1, al principio de `bits`. Devuelve cuántos escribió.
pub fn entrelazar(palabras: &[u8], bits: &mut [u8]) -> Result<usize, Error> {
    let profundidad = palabras.len();
    let total = profundidad.checked_mul(BITS).ok_or(Error::SalidaCorta)?;
    let destino = bits.get_mut(..total).ok_or(Error::SalidaCorta)?;
    if profundidad == 0 {
        return Ok(0);
    }
    for (j, fila) in destino.chunks_exact_mut(profundidad).enumerate() {
        for (bit, &palabra) in fila.iter_mut().zip(palabras) {
            *bit = (palabra >> j) & 1;
        }
    }
    Ok(total)
}

/// Deshace [`entrelazar`]: lee `bits` (un múltiplo de 8, cada uno 0 o 1) y escribe las
/// `bits.len() / 8` palabras al principio de `palabras`. Devuelve cuántas escribió.
pub fn desentrelazar(bits: &[u8], palabras: &mut [u8]) -> Result<usize, Error> {
    if !bits.len().is_multiple_of(BITS) {
        return Err(Error::LargoNoMultiplo);
    }
    if bits.iter().any(|&b| b > 1) {
        return Err(Error::BitInvalido);
    }
    let profundidad = bits.len() / BITS;
    let destino = palabras.get_mut(..profundidad).ok_or(Error::SalidaCorta)?;
    if profundidad == 0 {
        return Ok(0);
    }
    destino.fill(0);
    for (j, fila) in bits.chunks_exact(profundidad).enumerate() {
        for (palabra, &bit) in destino.iter_mut().zip(fila) {
            *palabra |= bit << j;
        }
    }
    Ok(profundidad)
}

/// Lo mismo que [`desentrelazar`], pero con las correlaciones de la decisión blanda: el valor
/// `j` de cada palabra de salida es el del bit `j`, listo para
/// [`decodificar_blando`](crate::hamming::decodificar_blando).
pub fn desentrelazar_blandos(blandos: &[i64], palabras: &mut [[i64; 8]]) -> Result<usize, Error> {
    if !blandos.len().is_multiple_of(BITS) {
        return Err(Error::LargoNoMultiplo);
    }
    let profundidad = blandos.len() / BITS;
    let destino = palabras.get_mut(..profundidad).ok_or(Error::SalidaCorta)?;
    if profundidad == 0 {
        return Ok(0);
    }
    for (j, fila) in blandos.chunks_exact(profundidad).enumerate() {
        for (palabra, &valor) in destino.iter_mut().zip(fila) {
            if let Some(celda) = palabra.get_mut(j) {
                *celda = valor;
            }
        }
    }
    Ok(profundidad)
}
