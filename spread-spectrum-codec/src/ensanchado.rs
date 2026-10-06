//! Esparcir y desesparcir: cada bit viaja como `N` chips, y el receptor correla.
//!
//! El transmisor multiplica el bit —en modulación de fase binaria, `+1` para el 0 y `−1` para
//! el 1— por la secuencia de chips y por la amplitud. El receptor suma cada bloque de `N`
//! muestras ponderado por los mismos chips: el bit transmitido aporta `±N · amplitud`, y lo
//! que no se parece a la secuencia —un tono, el ruido— aporta mucho menos.
//!
//! - [`correlar`] entrega la **decisión blanda**: la correlación de cada bit, con signo y
//!   magnitud (positiva para el 0, negativa para el 1). Es la entrada de
//!   [`decodificar_blando`](crate::hamming::decodificar_blando).
//! - [`decidir`] entrega la **decisión dura**: solo el signo, como bit 0 o 1. Una
//!   correlación de exactamente cero se decide como 0.
//! - [`recortar`] limita los valores blandos a lo que vale un bit limpio (`N · amplitud`),
//!   para que una ráfaga enorme no pese en la decisión blanda más que un bit seguro.
//!
//! Todo con enteros: las muestras son `i32` y las sumas `i64`, que no pueden desbordar con
//! códigos de hasta 127 chips.

use crate::lfsr::{chip_de, Codigo};
use crate::Error;

/// Esparce `bits` (cada uno 0 o 1) con `codigo` y `amplitud`, y escribe `bits.len() · N`
/// muestras al principio de `salida`. Devuelve cuántas escribió.
///
/// Si algún bit no es 0 ni 1, o si `salida` no alcanza, devuelve el error sin escribir nada.
pub fn esparcir(
    bits: &[u8],
    codigo: &Codigo,
    amplitud: i32,
    salida: &mut [i32],
) -> Result<usize, Error> {
    if bits.iter().any(|&b| b > 1) {
        return Err(Error::BitInvalido);
    }
    let chips = codigo.chips();
    let n = chips.len();
    if n == 0 {
        return Ok(0);
    }
    let total = bits.len().checked_mul(n).ok_or(Error::SalidaCorta)?;
    let destino = salida.get_mut(..total).ok_or(Error::SalidaCorta)?;
    for (bloque, &bit) in destino.chunks_exact_mut(n).zip(bits) {
        let signo = i32::from(chip_de(bit));
        for (muestra, &chip) in bloque.iter_mut().zip(chips) {
            *muestra = amplitud.saturating_mul(signo * i32::from(chip));
        }
    }
    Ok(total)
}

/// Decisión blanda: la correlación de cada bloque de `N` muestras con `codigo`, escrita al
/// principio de `salida`. Devuelve cuántos bits leyó.
///
/// `muestras` tiene que traer un número entero de bits; si no, [`Error::LargoNoMultiplo`].
pub fn correlar(muestras: &[i32], codigo: &Codigo, salida: &mut [i64]) -> Result<usize, Error> {
    let chips = codigo.chips();
    let simbolos = simbolos_en(muestras, chips.len())?;
    if simbolos == 0 {
        return Ok(0);
    }
    let destino = salida.get_mut(..simbolos).ok_or(Error::SalidaCorta)?;
    for (valor, bloque) in destino.iter_mut().zip(muestras.chunks_exact(chips.len())) {
        *valor = correlacion(bloque, chips);
    }
    Ok(simbolos)
}

/// Decisión dura: el bit (0 o 1) de cada bloque de `N` muestras, escrito al principio de
/// `bits`. Devuelve cuántos bits leyó.
pub fn decidir(muestras: &[i32], codigo: &Codigo, bits: &mut [u8]) -> Result<usize, Error> {
    let chips = codigo.chips();
    let simbolos = simbolos_en(muestras, chips.len())?;
    if simbolos == 0 {
        return Ok(0);
    }
    let destino = bits.get_mut(..simbolos).ok_or(Error::SalidaCorta)?;
    for (bit, bloque) in destino.iter_mut().zip(muestras.chunks_exact(chips.len())) {
        *bit = if correlacion(bloque, chips) >= 0 {
            0
        } else {
            1
        };
    }
    Ok(simbolos)
}

/// Recorta cada valor blando al intervalo `[−tope, tope]`.
///
/// Con `tope = N · amplitud`, lo que vale un bit limpio, un bit tapado por una ráfaga de ruido
/// enorme no puede arrastrar él solo la decisión blanda de toda su palabra. Un `tope` negativo
/// cuenta por su valor absoluto.
pub fn recortar(blandos: &mut [i64], tope: i64) {
    let tope = tope.saturating_abs();
    for valor in blandos {
        *valor = (*valor).clamp(-tope, tope);
    }
}

/// Cuántos bits completos hay en `muestras` con códigos de `n` chips.
fn simbolos_en(muestras: &[i32], n: usize) -> Result<usize, Error> {
    if n == 0 {
        return Ok(0);
    }
    if !muestras.len().is_multiple_of(n) {
        return Err(Error::LargoNoMultiplo);
    }
    Ok(muestras.len() / n)
}

/// `Σ muestra · chip` sobre un bloque.
fn correlacion(bloque: &[i32], chips: &[i8]) -> i64 {
    bloque
        .iter()
        .zip(chips)
        .map(|(&m, &c)| i64::from(m) * i64::from(c))
        .sum()
}
