//! La cola de la normal, contra `2 * pnorm(-abs(z))` de R.

use signal_validator::normal::p_bilateral;

#[test]
fn coincide_con_r_dentro_de_la_cota_de_la_aproximacion() {
    let casos = [
        (0.0, 1.0),
        (0.5, 0.6170750774519739),
        (1.0, 0.3173105078629141),
        (1.5, 0.13361440253771614),
        (1.959963984540054, 0.050000000000000024),
        (2.5, 0.01241933065155227),
        (3.0, 0.002699796063260189),
        (3.5, 0.00046525815807104987),
        (4.0, 6.334248366623985e-05),
        (5.0, 5.733031437583878e-07),
        (6.0, 1.973175290075396e-09),
    ];
    for (z, de_r) in casos {
        let p = p_bilateral(z);
        // La fórmula 7.1.26 garantiza un error absoluto menor que 1,5·10⁻⁷.
        assert!((p - de_r).abs() < 1.5e-7, "z = {z}: {p} contra {de_r}");
    }
}

#[test]
fn es_simetrica_y_acotada() {
    for z in [0.1, 0.7, 1.3, 2.2, 3.9] {
        assert_eq!(p_bilateral(z), p_bilateral(-z));
    }
    assert!(p_bilateral(0.0) <= 1.0);
    assert_eq!(p_bilateral(f64::INFINITY), 0.0);
    assert!(p_bilateral(f64::NAN).is_nan());
    // Decrece con |z|.
    let valores: Vec<f64> = (0..80).map(|i| p_bilateral(i as f64 * 0.1)).collect();
    assert!(valores.windows(2).all(|w| w[1] <= w[0]));
}
