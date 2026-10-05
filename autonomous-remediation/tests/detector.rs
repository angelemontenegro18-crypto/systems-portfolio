//! El detector CUSUM sobre llegadas sintéticas.
//!
//! Las series salen de un generador con semilla fija (ver `comun/llegadas.rs`):
//! los resultados son exactos y se repiten en cualquier plataforma. Las
//! tolerancias de cada test dejan margen sobre lo medido.

mod comun;

use std::f64::consts::LN_2;

use autonomous_remediation::detector::{Cusum, Sentido};
use comun::llegadas::tramos;

/// Muestras de referencia, con el servicio sano.
const REFERENCIA: usize = 1000;
/// Límite de la suma.
const LIMITE: f64 = 10.0;

/// Analiza lo que sigue a la referencia, con la referencia como base.
fn analizar(serie: &[u64]) -> autonomous_remediation::detector::Analisis {
    let (referencia, resto) = serie.split_at(REFERENCIA);
    Cusum::con_referencia(referencia, LIMITE)
        .expect("referencia válida")
        .analizar(resto)
}

fn mediana(mut v: Vec<usize>) -> usize {
    v.sort_unstable();
    v[v.len() / 2]
}

#[test]
fn ejemplo_a_mano() {
    // Media 10: cada intervalo de 40 suma −ln 2 + 2 a la suma «más lento».
    let lento = Cusum::nuevo(10.0, 2.0)
        .expect("válido")
        .analizar(&[10, 10, 40, 40]);
    let alarma = lento.alarma.expect("alarma");
    assert_eq!((alarma.indice, alarma.sentido), (3, Sentido::MasLento));
    assert_eq!(alarma.estadistico, (0.0 - LN_2 + 2.0) - LN_2 + 2.0);
    assert_eq!(lento.persistencia, 1.0);

    // Cada intervalo de 1 suma ln 2 − 0.1 a la suma «más rápido».
    let rapido = Cusum::nuevo(10.0, 1.0)
        .expect("válido")
        .analizar(&[10, 10, 1, 1]);
    let alarma = rapido.alarma.expect("alarma");
    assert_eq!((alarma.indice, alarma.sentido), (3, Sentido::MasRapido));
    assert_eq!(alarma.estadistico, 1.1862943611198906);
}

/// 200 series sanas de 2000 intervalos después de la referencia.
#[test]
fn con_el_servicio_sano_casi_no_hay_falsas_alarmas() {
    let falsas = (0..200u64)
        .filter(|s| {
            analizar(&tramos(1000 + s, &[(REFERENCIA, 20.0), (2000, 20.0)]))
                .alarma
                .is_some()
        })
        .count();
    // Medido: 1 de 200.
    assert!(falsas <= 4, "{falsas} de 200 series con falsa alarma");
}

/// El ritmo baja a un tercio (media de 20 ms a 60 ms) a los 300 intervalos.
#[test]
fn detecta_que_el_ritmo_baja() {
    let mut demoras = Vec::new();
    let mut antes_del_cambio = 0;
    for s in 0..200u64 {
        let analisis = analizar(&tramos(
            5000 + s,
            &[(REFERENCIA, 20.0), (300, 20.0), (700, 60.0)],
        ));
        let alarma = analisis.alarma.expect("todas las series dan la alarma");
        if alarma.indice < 300 {
            antes_del_cambio += 1;
            continue;
        }
        assert_eq!(alarma.sentido, Sentido::MasLento);
        demoras.push(alarma.indice - 300);
    }
    // Medido: 1 alarma antes del cambio; demora mediana 12, máxima 36.
    assert!(antes_del_cambio <= 3, "{antes_del_cambio}");
    assert!(
        *demoras.iter().max().expect("hay demoras") <= 50,
        "{demoras:?}"
    );
    assert!(mediana(demoras) <= 16);
}

/// El ritmo se triplica (media de 20 ms a 7 ms) a los 300 intervalos.
#[test]
fn detecta_que_el_ritmo_sube() {
    let mut demoras = Vec::new();
    for s in 0..200u64 {
        let analisis = analizar(&tramos(
            5000 + s,
            &[(REFERENCIA, 20.0), (300, 20.0), (700, 7.0)],
        ));
        let alarma = analisis.alarma.expect("todas las series dan la alarma");
        if alarma.indice >= 300 {
            assert_eq!(alarma.sentido, Sentido::MasRapido);
            demoras.push(alarma.indice - 300);
        }
    }
    // Medido: demora mediana 26, máxima 44.
    assert!(demoras.len() >= 197);
    assert!(
        *demoras.iter().max().expect("hay demoras") <= 60,
        "{demoras:?}"
    );
    assert!(mediana(demoras) <= 32);
}

/// Un cambio sostenido deja la suma por encima del límite; una ráfaga de 25
/// intervalos lentos, no.
#[test]
fn un_cambio_sostenido_persiste_y_una_rafaga_no() {
    let mut sostenidos = Vec::new();
    let mut pasajeros = Vec::new();
    for s in 0..100u64 {
        let sostenido = analizar(&tramos(
            7000 + s,
            &[(REFERENCIA, 20.0), (300, 20.0), (300, 60.0)],
        ));
        assert!(sostenido.alarma.is_some());
        sostenidos.push(sostenido.persistencia);
        let pasajero = analizar(&tramos(
            8000 + s,
            &[(REFERENCIA, 20.0), (300, 20.0), (25, 60.0), (275, 20.0)],
        ));
        if pasajero.alarma.is_some() {
            pasajeros.push(pasajero.persistencia);
        }
    }
    // Medido: sostenidos con persistencia mínima 0.78; pasajeros con mediana 0.19.
    let minima = sostenidos.iter().copied().fold(1.0, f64::min);
    assert!(
        minima >= 0.75,
        "persistencia mínima de un cambio sostenido: {minima}"
    );
    pasajeros.sort_by(f64::total_cmp);
    let mediana_pasajeros = pasajeros[pasajeros.len() / 2];
    assert!(
        mediana_pasajeros <= 0.3,
        "persistencia mediana de una ráfaga: {mediana_pasajeros}"
    );
}

/// Los dos escenarios de la demo, contra una réplica independiente escrita en
/// Python: mismos generador, referencia y sumas, mismos números hasta el
/// último dígito.
#[test]
fn coincide_con_una_replica_independiente() {
    let sostenido = tramos(2026, &[(REFERENCIA, 20.0), (300, 20.0), (300, 60.0)]);
    let cusum = Cusum::con_referencia(&sostenido[..REFERENCIA], LIMITE).expect("válida");
    assert_eq!(cusum.media_de_referencia(), 20.099);
    let analisis = cusum.analizar(&sostenido[REFERENCIA..]);
    let alarma = analisis.alarma.expect("alarma");
    assert_eq!(
        (alarma.indice, alarma.sentido, alarma.estadistico),
        (313, Sentido::MasLento, 12.980230490722601)
    );
    assert_eq!(analisis.persistencia, 1.0);

    let pasajero = tramos(
        2027,
        &[(REFERENCIA, 20.0), (300, 20.0), (25, 60.0), (275, 20.0)],
    );
    let cusum = Cusum::con_referencia(&pasajero[..REFERENCIA], LIMITE).expect("válida");
    assert_eq!(cusum.media_de_referencia(), 20.134);
    let analisis = cusum.analizar(&pasajero[REFERENCIA..]);
    let alarma = analisis.alarma.expect("alarma");
    assert_eq!(
        (alarma.indice, alarma.sentido, alarma.estadistico),
        (307, Sentido::MasLento, 11.664517598730928)
    );
    assert_eq!(analisis.persistencia, 0.5051194539249146);
}

#[test]
fn construccion() {
    assert!(Cusum::nuevo(0.0, 10.0).is_none());
    assert!(Cusum::nuevo(20.0, 0.0).is_none());
    assert!(Cusum::nuevo(f64::NAN, 10.0).is_none());
    assert!(Cusum::nuevo(20.0, f64::INFINITY).is_none());
    assert!(Cusum::con_referencia(&[], 10.0).is_none());
    assert!(Cusum::con_referencia(&[0, 0, 0], 10.0).is_none());
    let sin_datos = Cusum::nuevo(20.0, 10.0).expect("válido").analizar(&[]);
    assert!(sin_datos.alarma.is_none() && sin_datos.persistencia == 0.0);
}
