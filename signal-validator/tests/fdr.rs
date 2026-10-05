//! Benjamini–Hochberg.

use signal_validator::azar::Generador;
use signal_validator::fdr::{benjamini_hochberg, p_ajustados, ErrorFdr};
use signal_validator::normal::p_bilateral;

/// Umbrales k·q/m con q = 0,05 y m = 4: 0,0125 · 0,025 · 0,0375 · 0,05. Pasan
/// los dos primeros. Un umbral fijo de 0,05 rechazaría también 0,04; Bonferroni
/// (0,0125), solo el primero.
#[test]
fn ejemplo_a_mano() {
    let p = [0.001, 0.02, 0.04, 0.3];
    assert_eq!(
        benjamini_hochberg(&p, 0.05).expect("válido"),
        [true, true, false, false]
    );
    // El orden de entrada no importa.
    let p = [0.3, 0.04, 0.001, 0.02];
    assert_eq!(
        benjamini_hochberg(&p, 0.05).expect("válido"),
        [false, false, true, true]
    );
}

/// El procedimiento es de subida: p(2) = 0,03 no pasa su umbral (0,025), pero
/// p(4) = 0,04 pasa el suyo (0,05) y arrastra a todos los anteriores.
#[test]
fn es_de_subida() {
    let p = [0.01, 0.03, 0.035, 0.04];
    assert_eq!(
        benjamini_hochberg(&p, 0.05).expect("válido"),
        [true, true, true, true]
    );
}

/// Contra `p.adjust(p, method = "BH")` de R.
#[test]
fn coincide_con_r() {
    let p = [
        0.04, 0.001, 0.30, 0.02, 0.0105, 0.75, 0.049, 0.013, 0.2, 0.0001,
    ];
    let de_r = [
        0.06666666666666667,
        0.005,
        0.3333333333333333,
        0.04,
        0.0325,
        0.75,
        0.07,
        0.0325,
        0.25,
        0.001,
    ];
    let ajustados = p_ajustados(&p).expect("válido");
    for (a, r) in ajustados.iter().zip(de_r) {
        assert!((a - r).abs() <= 1e-15, "{a} contra {r}");
    }
    let rechazos = |q: f64| -> Vec<usize> {
        let r = benjamini_hochberg(&p, q).expect("válido");
        (0..p.len()).filter(|&i| r[i]).collect()
    };
    assert_eq!(rechazos(0.05), [1, 3, 4, 7, 9]);
    assert_eq!(rechazos(0.10), [0, 1, 3, 4, 6, 7, 9]);
}

#[test]
fn rechazar_por_ajustado_equivale_al_procedimiento() {
    let mut g = Generador::nuevo(4);
    for _ in 0..500 {
        let m = 1 + g.entero_bajo(40) as usize;
        // Mezcla de p-valores chicos y uniformes, con algún empate.
        let p: Vec<f64> = (0..m)
            .map(|_| match g.entero_bajo(4) {
                0 => g.uniforme() * 0.01,
                1 => 0.02,
                _ => g.uniforme(),
            })
            .collect();
        let ajustados = p_ajustados(&p).expect("válido");
        for q in [0.01, 0.05, 0.1, 0.2] {
            let rechazos = benjamini_hochberg(&p, q).expect("válido");
            for i in 0..m {
                assert_eq!(
                    rechazos[i],
                    ajustados[i] <= q,
                    "p = {p:?}, q = {q}, i = {i}"
                );
            }
        }
    }
}

/// Simulación: 100 hipótesis por repetición, 90 nulas y 10 con efecto real.
/// Cada una es una prueba z sobre la media de 20 valores normales.
fn simular(repeticiones: usize, con_efecto: usize, efecto: f64, semilla: u64) -> (f64, f64, f64) {
    let (m, n, q) = (100, 20, 0.05);
    let mut g = Generador::nuevo(semilla);
    let (mut fdp_bh, mut fdp_fijo, mut falsos_fijo) = (0.0, 0.0, 0.0);
    for _ in 0..repeticiones {
        let p: Vec<f64> = (0..m)
            .map(|h| {
                let media = if h < con_efecto { efecto } else { 0.0 };
                let suma: f64 = (0..n).map(|_| media + g.normal()).sum();
                p_bilateral(suma / (n as f64).sqrt())
            })
            .collect();
        let bh = benjamini_hochberg(&p, q).expect("válido");
        let fijo: Vec<bool> = p.iter().map(|&v| v <= q).collect();
        let proporcion_falsa = |r: &[bool]| {
            let declarados = r.iter().filter(|&&x| x).count();
            let falsos = r[con_efecto..].iter().filter(|&&x| x).count();
            if declarados == 0 {
                0.0
            } else {
                falsos as f64 / declarados as f64
            }
        };
        fdp_bh += proporcion_falsa(&bh);
        fdp_fijo += proporcion_falsa(&fijo);
        falsos_fijo += fijo[con_efecto..].iter().filter(|&&x| x).count() as f64;
    }
    let r = repeticiones as f64;
    (fdp_bh / r, fdp_fijo / r, falsos_fijo / r)
}

/// La FDR medida queda en el nivel pedido; con umbral fijo, muy por encima.
#[test]
fn controla_la_fdr_en_simulacion() {
    let (fdr_bh, fdr_fijo, _) = simular(300, 10, 0.8, 21);
    // Teoría: q·m₀/m = 0,045. Margen de 0,025 por ser una estimación.
    assert!(fdr_bh <= 0.07, "FDR con BH: {fdr_bh}");
    assert!(fdr_fijo >= 0.2, "FDR con umbral fijo: {fdr_fijo}");
}

/// Con todas las hipótesis nulas, el umbral fijo «descubre» unas cinco por
/// repetición; BH casi nunca descubre nada.
#[test]
fn cien_nulas_dan_cinco_falsos_descubrimientos_sin_corregir() {
    let (fdr_bh, _, falsos_fijo) = simular(300, 0, 0.0, 22);
    assert!(
        (4.0..=6.0).contains(&falsos_fijo),
        "falsos por repetición: {falsos_fijo}"
    );
    // Con todas nulas, la FDR es la probabilidad de declarar algo.
    assert!(fdr_bh <= 0.08, "FDR con BH: {fdr_bh}");
}

#[test]
fn errores() {
    // NaN no es igual a sí mismo, así que se compara la forma, no el valor.
    assert!(matches!(
        benjamini_hochberg(&[0.1, f64::NAN], 0.05),
        Err(ErrorFdr::PValorInvalido { indice: 1, .. })
    ));
    assert!(matches!(
        benjamini_hochberg(&[1.5], 0.05),
        Err(ErrorFdr::PValorInvalido { indice: 0, .. })
    ));
    assert!(matches!(
        benjamini_hochberg(&[-0.1], 0.05),
        Err(ErrorFdr::PValorInvalido { indice: 0, .. })
    ));
    assert!(matches!(
        p_ajustados(&[0.2, 2.0]),
        Err(ErrorFdr::PValorInvalido { indice: 1, .. })
    ));
    for q in [0.0, 1.0, -0.1, f64::NAN] {
        assert!(
            matches!(
                benjamini_hochberg(&[0.1], q),
                Err(ErrorFdr::NivelInvalido { .. })
            ),
            "q = {q}"
        );
    }
    assert_eq!(benjamini_hochberg(&[], 0.05), Ok(Vec::new()));
    assert_eq!(p_ajustados(&[]), Ok(Vec::new()));
}
