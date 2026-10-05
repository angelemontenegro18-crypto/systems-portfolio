//! Cola de la normal estándar, para convertir un estadístico z en p-valor.

/// p-valor bilateral de un estadístico z: `P(|Z| ≥ |z|)` para `Z` normal
/// estándar, que es `erfc(|z| / √2)`.
///
/// `erfc` se aproxima con la fórmula 7.1.26 de Abramowitz y Stegun (*Handbook
/// of Mathematical Functions*, 1964), de error absoluto menor que 1,5·10⁻⁷.
/// Alcanza para decidir con umbrales del orden de 10⁻⁴; no para p-valores
/// minúsculos, donde el error relativo crece.
///
/// Un `z` NaN da NaN.
///
/// ```
/// use signal_validator::normal::p_bilateral;
///
/// assert!((p_bilateral(1.96) - 0.05).abs() < 1e-3);
/// assert!((p_bilateral(0.0) - 1.0).abs() < 1e-6);
/// ```
pub fn p_bilateral(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    erfc_no_negativo(z.abs() / std::f64::consts::SQRT_2).min(1.0)
}

/// `erfc(x)` para `x ≥ 0`, con la fórmula 7.1.26.
fn erfc_no_negativo(x: f64) -> f64 {
    const P: f64 = 0.327_591_1;
    const A: [f64; 5] = [
        0.254_829_592,
        -0.284_496_736,
        1.421_413_741,
        -1.453_152_027,
        1.061_405_429,
    ];
    let t = 1.0 / (1.0 + P * x);
    let polinomio = t * (A[0] + t * (A[1] + t * (A[2] + t * (A[3] + t * A[4]))));
    polinomio * (-x * x).exp()
}
