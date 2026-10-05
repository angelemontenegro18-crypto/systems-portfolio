//! La prueba de permutación por bloques.

use signal_validator::azar::Generador;
use signal_validator::permutacion::{correlacion, prueba_por_bloques, ErrorPrueba};
use signal_validator::sintetico::{ar1, senal_plantada};

fn dos_colas(x: &[f64], y: &[f64]) -> f64 {
    correlacion(x, y).abs()
}

/// Ninguna permutación alcanza una relación perfecta, y aun así el p-valor
/// es 1/(B+1), no cero.
#[test]
fn el_p_valor_nunca_es_cero() {
    let x: Vec<f64> = (0..200).map(|i| (i as f64 * 0.37).sin()).collect();
    let prueba =
        prueba_por_bloques(&x, &x, 10, 999, &mut Generador::nuevo(1), dos_colas).expect("válida");
    assert_eq!(prueba.al_menos_tan_extremas, 0);
    assert_eq!(prueba.p_valor, 1.0 / 1000.0);
    assert_eq!(prueba.bloques, 20);
}

#[test]
fn el_p_valor_cuenta_al_observado_como_una_permutacion_mas() {
    let mut g = Generador::nuevo(8);
    for _ in 0..20 {
        let x = ar1(120, 0.3, &mut g);
        let y = ar1(120, 0.3, &mut g);
        let b = 1 + g.entero_bajo(300) as usize;
        let prueba = prueba_por_bloques(&x, &y, 7, b, &mut g, dos_colas).expect("válida");
        assert_eq!(
            prueba.p_valor,
            (1 + prueba.al_menos_tan_extremas) as f64 / (1 + b) as f64
        );
        assert!(prueba.p_valor >= 1.0 / (b + 1) as f64 && prueba.p_valor <= 1.0);
    }
}

/// Si el estadístico permutado sale NaN, cuenta en contra: el p-valor sube.
#[test]
fn un_estadistico_permutado_nan_cuenta_como_extremo() {
    let x: Vec<f64> = (0..100).map(|i| i as f64).collect();
    let original = x.clone();
    let solo_el_observado = move |_: &[f64], y: &[f64]| {
        if y == original.as_slice() {
            0.5
        } else {
            f64::NAN
        }
    };
    let prueba = prueba_por_bloques(&x, &x, 5, 99, &mut Generador::nuevo(2), solo_el_observado)
        .expect("válida");
    assert_eq!(prueba.al_menos_tan_extremas, 99);
    assert_eq!(prueba.p_valor, 1.0);
}

/// Dos series AR(1) independientes con mucha memoria (φ = 0,8). Permutando de a
/// un valor, la referencia del azar queda demasiado angosta y se rechaza mucho
/// más que el 5 % prometido; por bloques, no.
#[test]
fn con_dependencia_permutar_de_a_uno_da_falsos_positivos_y_por_bloques_no() {
    let mut g = Generador::nuevo(77);
    let repeticiones = 200;
    let (mut de_a_uno, mut por_bloques) = (0, 0);
    for _ in 0..repeticiones {
        let x = ar1(500, 0.8, &mut g);
        let y = ar1(500, 0.8, &mut g);
        if prueba_por_bloques(&x, &y, 1, 99, &mut g, dos_colas)
            .expect("válida")
            .p_valor
            <= 0.05
        {
            de_a_uno += 1;
        }
        if prueba_por_bloques(&x, &y, 25, 99, &mut g, dos_colas)
            .expect("válida")
            .p_valor
            <= 0.05
        {
            por_bloques += 1;
        }
    }
    let tasa_uno = de_a_uno as f64 / repeticiones as f64;
    let tasa_bloques = por_bloques as f64 / repeticiones as f64;
    // Nominal: 0,05. De a uno, lo medido ronda 0,3; por bloques, 0,05.
    assert!(tasa_uno >= 0.20, "de a uno rechazó solo {tasa_uno}");
    assert!(tasa_bloques <= 0.10, "por bloques rechazó {tasa_bloques}");
}

/// Una señal débil (correlación ≈ 0,15) en una de cinco candidatas: la
/// prueba la encuentra.
#[test]
fn detecta_una_senal_debil_plantada() {
    let mut g = Generador::nuevo(31);
    let escenario = senal_plantada(2000, 5, 2, 0.15, &mut g);
    let plantada = &escenario.candidatas[escenario.plantada];
    assert!(
        correlacion(plantada, &escenario.etiqueta) < 0.25,
        "la señal es débil"
    );
    let prueba = prueba_por_bloques(plantada, &escenario.etiqueta, 50, 399, &mut g, dos_colas)
        .expect("válida");
    assert!(prueba.p_valor <= 0.01, "p = {}", prueba.p_valor);
}

#[test]
fn errores() {
    let x = [1.0, 2.0, 3.0, 4.0];
    let mut g = Generador::nuevo(0);
    assert_eq!(
        prueba_por_bloques(&x, &x[..3], 1, 10, &mut g, dos_colas),
        Err(ErrorPrueba::LargosDistintos { x: 4, y: 3 })
    );
    assert_eq!(
        prueba_por_bloques(&x, &x, 0, 10, &mut g, dos_colas),
        Err(ErrorPrueba::BloqueVacio)
    );
    assert_eq!(
        prueba_por_bloques(&x, &x, 4, 10, &mut g, dos_colas),
        Err(ErrorPrueba::MenosDeDosBloques { bloques: 1 })
    );
    assert_eq!(
        prueba_por_bloques(&x, &x, 1, 0, &mut g, dos_colas),
        Err(ErrorPrueba::SinPermutaciones)
    );
    let constante = [5.0; 4];
    assert_eq!(
        prueba_por_bloques(&constante, &x, 1, 10, &mut g, dos_colas),
        Err(ErrorPrueba::EstadisticoIndefinido)
    );
}

#[test]
fn la_correlacion() {
    assert_eq!(correlacion(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), 1.0);
    assert_eq!(correlacion(&[1.0, 2.0, 3.0], &[6.0, 4.0, 2.0]), -1.0);
    assert!(correlacion(&[1.0, 2.0, 3.0], &[1.0, 1.0, 1.0]).is_nan());
    assert!(correlacion(&[1.0, 2.0], &[1.0]).is_nan());
    assert!(correlacion(&[1.0], &[1.0]).is_nan());
    // Un ejemplo sin simetrías: x = 1..5, y = (2, 1, 4, 3, 5) → r = 0,8.
    let r = correlacion(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 1.0, 4.0, 3.0, 5.0]);
    assert!((r - 0.8).abs() < 1e-12, "r = {r}");
}
