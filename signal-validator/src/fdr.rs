//! Muchas hipótesis a la vez: el procedimiento de Benjamini y Hochberg.
//!
//! Con 100 hipótesis sin ningún efecto real y un umbral fijo de 0,05, salen en
//! promedio cinco «descubrimientos», todos falsos. Benjamini y Hochberg (1995,
//! *Journal of the Royal Statistical Society, Series B*) propusieron controlar
//! otra cosa: la **proporción esperada de descubrimientos falsos** (FDR) entre
//! los que se declaran. Su procedimiento, con `m` p-valores ordenados de menor a
//! mayor `p(1) ≤ … ≤ p(m)`:
//!
//! 1. buscar el mayor `k` tal que `p(k) ≤ k·q/m`;
//! 2. rechazar las hipótesis de `p(1)` a `p(k)`.
//!
//! Es un procedimiento **de subida**: si `p(k)` pasa su umbral, se rechazan
//! también todas las anteriores, aunque alguna no pasara el suyo.

use std::fmt;

/// Por qué no se pudo aplicar el procedimiento.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorFdr {
    /// Un p-valor es NaN o está fuera de `[0, 1]`.
    PValorInvalido {
        /// Su posición.
        indice: usize,
        /// Su valor.
        valor: f64,
    },
    /// El nivel `q` no está en `(0, 1)`.
    NivelInvalido {
        /// El nivel pedido.
        q: f64,
    },
}

impl fmt::Display for ErrorFdr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorFdr::PValorInvalido { indice, valor } => {
                write!(f, "el p-valor {indice} ({valor}) no está en [0, 1]")
            }
            ErrorFdr::NivelInvalido { q } => write!(f, "el nivel q = {q} no está en (0, 1)"),
        }
    }
}

impl std::error::Error for ErrorFdr {}

fn validar(p_valores: &[f64]) -> Result<(), ErrorFdr> {
    match p_valores.iter().position(|p| !(0.0..=1.0).contains(p)) {
        Some(indice) => Err(ErrorFdr::PValorInvalido {
            indice,
            valor: p_valores[indice],
        }),
        None => Ok(()),
    }
}

/// Índices ordenados por p-valor creciente; los empates, por posición.
fn orden_creciente(p_valores: &[f64]) -> Vec<usize> {
    let mut orden: Vec<usize> = (0..p_valores.len()).collect();
    orden.sort_by(|&a, &b| p_valores[a].total_cmp(&p_valores[b]).then(a.cmp(&b)));
    orden
}

/// Qué hipótesis rechazar con FDR controlada al nivel `q`.
///
/// Devuelve un `bool` por p-valor, en el mismo orden de entrada.
///
/// ```
/// use signal_validator::fdr::benjamini_hochberg;
///
/// // Umbrales k·q/m con q = 0,05 y m = 4: 0,0125 · 0,025 · 0,0375 · 0,05.
/// let p = [0.001, 0.02, 0.04, 0.3];
/// assert_eq!(benjamini_hochberg(&p, 0.05).unwrap(), [true, true, false, false]);
/// ```
pub fn benjamini_hochberg(p_valores: &[f64], q: f64) -> Result<Vec<bool>, ErrorFdr> {
    validar(p_valores)?;
    if !(q > 0.0 && q < 1.0) {
        return Err(ErrorFdr::NivelInvalido { q });
    }
    let m = p_valores.len();
    let orden = orden_creciente(p_valores);

    // El mayor rango k con p(k) ≤ k·q/m, comparado como p(k)·m ≤ k·q para no
    // dividir. Se recorre de arriba hacia abajo: el primero que pasa es el mayor.
    let k = (1..=m)
        .rev()
        .find(|&rango| p_valores[orden[rango - 1]] * m as f64 <= rango as f64 * q)
        .unwrap_or(0);

    let mut rechazos = vec![false; m];
    for &i in &orden[..k] {
        rechazos[i] = true;
    }
    Ok(rechazos)
}

/// p-valores ajustados por Benjamini–Hochberg: el menor nivel `q` al que cada
/// hipótesis se rechazaría.
///
/// `ajustado(i) = min(1, min_{j ≥ rango(i)} p(j)·m/j)`. Rechazar las de
/// `ajustado ≤ q` da lo mismo que [`benjamini_hochberg`] con ese `q`.
pub fn p_ajustados(p_valores: &[f64]) -> Result<Vec<f64>, ErrorFdr> {
    validar(p_valores)?;
    let m = p_valores.len();
    let orden = orden_creciente(p_valores);
    let mut ajustados = vec![0.0; m];
    let mut minimo = 1.0f64;
    for rango in (1..=m).rev() {
        let i = orden[rango - 1];
        minimo = minimo.min(p_valores[i] * m as f64 / rango as f64);
        ajustados[i] = minimo;
    }
    Ok(ajustados)
}
