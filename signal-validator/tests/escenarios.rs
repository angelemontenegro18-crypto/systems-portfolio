//! El problema que el crate resuelve, medido sobre datos sintéticos.

use signal_validator::azar::Generador;
use signal_validator::particion::{kfold_barajado, particionar};
use signal_validator::sintetico::{acierto_por_vecinos, ruido, ventanas_pasadas};

/// Ruido puro: ninguna característica puede predecir la etiqueta, así que el
/// acierto verdadero de cualquier modelo es 50 %. Con pliegues barajados, las
/// observaciones vecinas —casi idénticas— caen a ambos lados y el modelo
/// «acierta» de más. Con purga y embargo, vuelve a rondar el 50 %.
#[test]
fn sobre_ruido_el_kfold_barajado_inventa_senal_y_el_purgado_no() {
    let mut g = Generador::nuevo(1);
    let serie = ruido(3000, &mut g);
    let conjunto = ventanas_pasadas(&serie, &[20, 50, 100], 40);

    let barajados = kfold_barajado(conjunto.etiquetas.len(), 5, &mut g).expect("válido");
    let purgados = particionar(&conjunto.intervalos, 5, 40).expect("válido");
    let acierto_barajado = acierto_por_vecinos(&conjunto, &barajados, 5);
    let acierto_purgado = acierto_por_vecinos(&conjunto, &purgados, 5);

    // En 30 semillas, el barajado nunca bajó de 0,63 y el purgado quedó
    // entre 0,44 y 0,59.
    assert!(acierto_barajado >= 0.60, "barajado: {acierto_barajado}");
    assert!(
        (acierto_purgado - 0.5).abs() <= 0.10,
        "purgado: {acierto_purgado}"
    );
}

#[test]
fn las_ventanas_pasadas_miran_atras_y_adelante_lo_justo() {
    let serie: Vec<f64> = (0..10).map(|i| i as f64 - 4.5).collect();
    let conjunto = ventanas_pasadas(&serie, &[1, 3], 2);
    // t va de 2 (primera con ventana 3) a 7 (última con horizonte 2).
    assert_eq!(conjunto.etiquetas.len(), 6);
    // t = 2: ventanas [2] y [0, 1, 2] → -2,5 y -3,5; futuro 3 + 4 → -1,5 - 0,5 < 0.
    assert_eq!(conjunto.caracteristicas[0], vec![-2.5, -3.5]);
    assert_eq!(conjunto.etiquetas[0], -1.0);
    assert_eq!(
        (
            conjunto.intervalos[0].inicio(),
            conjunto.intervalos[0].fin()
        ),
        (0, 4)
    );
    // t = 7: futuro 8 + 9 → 3,5 + 4,5 > 0.
    assert_eq!(conjunto.etiquetas[5], 1.0);
    assert_eq!(
        (
            conjunto.intervalos[5].inicio(),
            conjunto.intervalos[5].fin()
        ),
        (5, 9)
    );
    // Una serie demasiado corta no da observaciones.
    assert!(ventanas_pasadas(&serie[..3], &[3], 2).etiquetas.is_empty());
}
