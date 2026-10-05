//! El generador: vectores de referencia y ausencia de sesgo.

use signal_validator::azar::Generador;

/// Primeras salidas para varias semillas, obtenidas con una implementación
/// independiente del mismo algoritmo (`java.util.SplittableRandom`, cuyo
/// `nextLong` es SplitMix64 con el mismo incremento).
#[test]
fn coincide_con_la_implementacion_de_referencia() {
    let casos: [(u64, [u64; 4]); 4] = [
        (
            0,
            [
                16294208416658607535,
                7960286522194355700,
                487617019471545679,
                17909611376780542444,
            ],
        ),
        (
            1,
            [
                10451216379200822465,
                13757245211066428519,
                17911839290282890590,
                8196980753821780235,
            ],
        ),
        (
            2026,
            [
                15824617304438902051,
                8699989649721214301,
                12310341597754734734,
                7097835237234771186,
            ],
        ),
        (
            u64::MAX,
            [
                16490336266968443936,
                16834447057089888969,
                4048727598324417001,
                7862637804313477842,
            ],
        ),
    ];
    for (semilla, esperadas) in casos {
        let mut g = Generador::nuevo(semilla);
        let salidas: Vec<u64> = (0..4).map(|_| g.siguiente()).collect();
        assert_eq!(salidas, esperadas, "semilla {semilla}");
    }
}

/// `uniforme` y `normal` dan los mismos bits que la referencia: `nextDouble`
/// de Java para la uniforme, y la suma de 12 de ellas menos 6 para la normal.
/// Es lo que garantiza que los datos sintéticos sean idénticos en cualquier
/// plataforma.
#[test]
fn uniforme_y_normal_son_reproducibles_bit_a_bit() {
    let mut g = Generador::nuevo(2026);
    let uniformes: Vec<f64> = (0..3).map(|_| g.uniforme()).collect();
    assert_eq!(
        uniformes,
        [0.8578542230112182, 0.4716273839414571, 0.667344955216218]
    );

    let mut g = Generador::nuevo(7);
    let bits: Vec<u64> = (0..4).map(|_| g.normal().to_bits()).collect();
    assert_eq!(
        bits,
        [
            0xbff003e872f99a30,
            0x3ff5562e6c43d4c0,
            0x3fe09a84ae14b608,
            0xbfec23cce405d678
        ]
    );
}

#[test]
fn la_normal_tiene_media_cero_y_varianza_uno() {
    let mut g = Generador::nuevo(11);
    let n = 200_000;
    let valores: Vec<f64> = (0..n).map(|_| g.normal()).collect();
    let media = valores.iter().sum::<f64>() / n as f64;
    let varianza = valores.iter().map(|v| (v - media).powi(2)).sum::<f64>() / n as f64;
    // Error estándar de la media: 1/√n ≈ 0,0022. Tolerancias de unas 5 veces eso.
    assert!(media.abs() < 0.012, "media {media}");
    assert!((varianza - 1.0).abs() < 0.015, "varianza {varianza}");
}

/// Con `n = 3·2⁶²`, el módulo ingenuo saca un resto menor que 2⁶² la mitad de
/// las veces en vez de un tercio: la franja que sobra es enorme. Sin sesgo,
/// sale un tercio.
#[test]
fn entero_bajo_no_tiene_sesgo_de_modulo() {
    let n = 3u64 << 62;
    let mut g = Generador::nuevo(5);
    let tiradas = 30_000;
    let bajos = (0..tiradas)
        .filter(|_| g.entero_bajo(n) < (1u64 << 62))
        .count();
    let fraccion = bajos as f64 / tiradas as f64;
    // Un tercio ± 0,02 (unas 7 veces el error estándar); el sesgado daría 0,5.
    assert!((fraccion - 1.0 / 3.0).abs() < 0.02, "fracción {fraccion}");
}

#[test]
fn entero_bajo_respeta_el_rango() {
    let mut g = Generador::nuevo(3);
    for n in [1u64, 2, 3, 7, 1000, u64::MAX] {
        for _ in 0..200 {
            assert!(g.entero_bajo(n) < n);
        }
    }
}

/// Las seis permutaciones de tres elementos salen con la misma frecuencia.
///
/// El error clásico —elegir el intercambio entre todas las posiciones en vez
/// de entre las que faltan— da 27 caminos para 6 permutaciones: unas salen
/// 4/27 de las veces y otras 5/27. Con 60 000 tiradas eso es 8 889 u 11 111
/// en vez de 10 000, bien afuera de la banda.
#[test]
fn barajar_da_todas_las_permutaciones_por_igual() {
    let mut g = Generador::nuevo(9);
    let mut cuentas = std::collections::BTreeMap::new();
    let tiradas = 60_000;
    for _ in 0..tiradas {
        let mut v = [0, 1, 2];
        g.barajar(&mut v);
        *cuentas.entry(v).or_insert(0usize) += 1;
    }
    assert_eq!(cuentas.len(), 6);
    for (permutacion, cuenta) in cuentas {
        // 10 000 esperadas; el desvío es ≈ 91, así que ±450 son unos 5 desvíos.
        assert!(
            (9_550..=10_450).contains(&cuenta),
            "{permutacion:?} salió {cuenta} veces"
        );
    }
}
