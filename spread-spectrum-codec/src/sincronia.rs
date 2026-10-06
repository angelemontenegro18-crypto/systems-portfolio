//! Sincronización de trama: dónde empieza la trama, buscando el preámbulo por correlación.
//!
//! El preámbulo es un código conocido (por ejemplo, la secuencia de máximo largo de grado 7)
//! transmitido como bits 0. El receptor desliza ese código por las muestras, una posición por
//! vez, hasta la primera cuya correlación alcanza el umbral, y desde ahí se queda con la más
//! alta de las `L − 1` siguientes: el **pico**. Si nada alcanza el umbral, no hay trama.
//!
//! Que mande la primera posición y no el máximo de todo el búfer es deliberado: una ráfaga
//! fuerte **después** del preámbulo puede correlar más que él, y no debe desplazarlo. Una
//! ráfaga fuerte **antes** del preámbulo sí puede engañar a la búsqueda; es un límite
//! conocido, con su prueba.
//!
//! Con el preámbulo completo y a la amplitud esperada, el pico vale `L · amplitud` (`L`
//! chips); [`umbral_de_media_altura`] pide la mitad, que deja margen para el ruido y descarta
//! una trama que llega a menos de la mitad de su amplitud.

use crate::lfsr::Codigo;

/// La posición donde la correlación con el preámbulo es máxima, y cuánto vale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pico {
    /// Índice de la muestra donde empieza el preámbulo.
    pub posicion: usize,
    /// La correlación en esa posición.
    pub correlacion: i64,
}

/// Busca `preambulo` en `muestras`: la primera posición cuya correlación alcanza `umbral`,
/// afinada al máximo de las `L − 1` posiciones siguientes (en empate, la primera). Si ninguna
/// lo alcanza, o si las muestras son más cortas que el preámbulo, `None`.
pub fn buscar_preambulo(muestras: &[i32], preambulo: &Codigo, umbral: i64) -> Option<Pico> {
    let chips = preambulo.chips();
    if chips.is_empty() {
        return None;
    }
    let mut picos = muestras
        .windows(chips.len())
        .enumerate()
        .map(|(posicion, ventana)| Pico {
            posicion,
            correlacion: ventana
                .iter()
                .zip(chips)
                .map(|(&m, &c)| i64::from(m) * i64::from(c))
                .sum(),
        });
    let mut mejor = picos.by_ref().find(|p| p.correlacion >= umbral)?;
    for p in picos.take(chips.len() - 1) {
        if p.correlacion > mejor.correlacion {
            mejor = p;
        }
    }
    Some(mejor)
}

/// El umbral de media altura: `L · amplitud / 2`, la mitad del pico que da el preámbulo
/// completo a esa amplitud.
pub fn umbral_de_media_altura(preambulo: &Codigo, amplitud: i32) -> i64 {
    let largo = i64::try_from(preambulo.largo()).unwrap_or(i64::MAX);
    largo.saturating_mul(i64::from(amplitud)) / 2
}
