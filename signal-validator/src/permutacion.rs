//! Prueba de permutación por bloques.
//!
//! La pregunta es si un estadístico —por ejemplo, la correlación entre una
//! característica y una etiqueta— es más grande de lo que daría el azar. La
//! respuesta se construye con los propios datos: se desarma la relación entre
//! `x` e `y` reordenando `y`, muchas veces, y se mira cuántas veces el
//! estadístico permutado alcanza al observado.
//!
//! En una serie temporal, reordenar `y` de a un valor destruye también su
//! dependencia interna, y la referencia queda demasiado angosta: aparecen
//! falsos positivos (el test
//! `con_dependencia_permutar_de_a_uno_da_falsos_positivos_y_por_bloques_no`
//! lo mide). Aquí se reordenan **bloques contiguos** de `y`, que conservan esa
//! dependencia por dentro. Es la idea del remuestreo por bloques para datos
//! dependientes (Künsch, 1989, *Annals of Statistics*; Politis y Romano, 1994,
//! *Journal of the American Statistical Association*), aplicada a una
//! permutación.
//!
//! El p-valor es `(1 + r) / (1 + B)`, con `r` las permutaciones que alcanzaron
//! al observado y `B` las que se hicieron: el dato observado cuenta como una
//! permutación más (Phipson y Smyth, 2010, *Statistical Applications in
//! Genetics and Molecular Biology*). Así el p-valor nunca es cero y la prueba
//! no rechaza más de lo que promete.

use std::fmt;

use crate::azar::Generador;

/// Resultado de una prueba de permutación.
#[derive(Debug, Clone, PartialEq)]
pub struct Prueba {
    /// El estadístico sobre los datos sin permutar.
    pub estadistico: f64,
    /// `(1 + al_menos_tan_extremas) / (1 + permutaciones)`.
    pub p_valor: f64,
    /// Cuántas permutaciones se hicieron.
    pub permutaciones: usize,
    /// Cuántas dieron un estadístico mayor o igual al observado (o NaN).
    pub al_menos_tan_extremas: usize,
    /// En cuántos bloques se cortó `y`. Con pocos bloques hay pocas
    /// permutaciones distintas y el p-valor es grueso.
    pub bloques: usize,
}

/// Por qué no se pudo hacer la prueba.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorPrueba {
    /// `x` e `y` tienen largos distintos.
    LargosDistintos {
        /// Largo de `x`.
        x: usize,
        /// Largo de `y`.
        y: usize,
    },
    /// El largo de bloque es cero.
    BloqueVacio,
    /// Con menos de dos bloques no hay nada que permutar.
    MenosDeDosBloques {
        /// Los bloques que salieron.
        bloques: usize,
    },
    /// Se pidieron cero permutaciones.
    SinPermutaciones,
    /// El estadístico observado es NaN: no hay con qué comparar.
    EstadisticoIndefinido,
}

impl fmt::Display for ErrorPrueba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorPrueba::LargosDistintos { x, y } => write!(f, "x tiene {x} valores e y tiene {y}"),
            ErrorPrueba::BloqueVacio => write!(f, "el largo de bloque debe ser al menos 1"),
            ErrorPrueba::MenosDeDosBloques { bloques } => {
                write!(f, "con {bloques} bloque(s) no hay nada que permutar")
            }
            ErrorPrueba::SinPermutaciones => write!(f, "hace falta al menos una permutación"),
            ErrorPrueba::EstadisticoIndefinido => write!(f, "el estadístico observado es NaN"),
        }
    }
}

impl std::error::Error for ErrorPrueba {}

/// Prueba de permutación por bloques, de una cola: ¿es `estadistico(x, y)`
/// más grande de lo que daría el azar?
///
/// `y` se corta en bloques contiguos de `largo_bloque` (el último puede quedar
/// más corto) y en cada permutación se reordenan los bloques; `x` no se toca.
/// Para una prueba de dos colas, el estadístico debe devolver un valor
/// absoluto, como `|x, y| correlacion(x, y).abs()`.
///
/// Un estadístico permutado NaN cuenta como «al menos tan extremo»: ante la
/// duda, el p-valor sube, nunca baja.
///
/// ```
/// use signal_validator::azar::Generador;
/// use signal_validator::permutacion::{correlacion, prueba_por_bloques};
///
/// let x: Vec<f64> = (0..200).map(|i| (i as f64 * 0.37).sin()).collect();
/// let y = x.clone(); // relación perfecta
/// let mut g = Generador::nuevo(1);
/// let prueba = prueba_por_bloques(&x, &y, 10, 999, &mut g, |a, b| correlacion(a, b).abs()).unwrap();
/// assert_eq!(prueba.p_valor, 1.0 / 1000.0); // nunca cero
/// ```
pub fn prueba_por_bloques<F>(
    x: &[f64],
    y: &[f64],
    largo_bloque: usize,
    permutaciones: usize,
    generador: &mut Generador,
    estadistico: F,
) -> Result<Prueba, ErrorPrueba>
where
    F: Fn(&[f64], &[f64]) -> f64,
{
    if x.len() != y.len() {
        return Err(ErrorPrueba::LargosDistintos {
            x: x.len(),
            y: y.len(),
        });
    }
    if largo_bloque == 0 {
        return Err(ErrorPrueba::BloqueVacio);
    }
    let bloques = y.len().div_ceil(largo_bloque);
    if bloques < 2 {
        return Err(ErrorPrueba::MenosDeDosBloques { bloques });
    }
    if permutaciones == 0 {
        return Err(ErrorPrueba::SinPermutaciones);
    }
    let observado = estadistico(x, y);
    if observado.is_nan() {
        return Err(ErrorPrueba::EstadisticoIndefinido);
    }

    let mut orden: Vec<usize> = (0..bloques).collect();
    let mut permutada = Vec::with_capacity(y.len());
    let mut al_menos_tan_extremas = 0usize;
    for _ in 0..permutaciones {
        // Fisher–Yates da una permutación uniforme partiendo de cualquier
        // orden, así que no hace falta volver a la identidad cada vez.
        generador.barajar(&mut orden);
        permutada.clear();
        for &b in &orden {
            let desde = b * largo_bloque;
            let hasta = (desde + largo_bloque).min(y.len());
            permutada.extend_from_slice(&y[desde..hasta]);
        }
        let valor = estadistico(x, &permutada);
        if valor >= observado || valor.is_nan() {
            al_menos_tan_extremas += 1;
        }
    }

    Ok(Prueba {
        estadistico: observado,
        p_valor: (1 + al_menos_tan_extremas) as f64 / (1 + permutaciones) as f64,
        permutaciones,
        al_menos_tan_extremas,
        bloques,
    })
}

/// Correlación de Pearson entre `x` e `y`.
///
/// Devuelve NaN si los largos difieren, si hay menos de dos valores o si
/// alguna de las dos series es constante: en esos casos no está definida.
pub fn correlacion(x: &[f64], y: &[f64]) -> f64 {
    if x.len() != y.len() || x.len() < 2 {
        return f64::NAN;
    }
    let n = x.len() as f64;
    let media_x = x.iter().sum::<f64>() / n;
    let media_y = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        let (dx, dy) = (a - media_x, b - media_y);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx == 0.0 || syy == 0.0 {
        return f64::NAN;
    }
    sxy / (sxx * syy).sqrt()
}
