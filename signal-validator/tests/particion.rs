//! La partición con purga y embargo.

use signal_validator::azar::Generador;
use signal_validator::particion::{
    kfold_barajado, particionar, ErrorParticion, Intervalo, Pliegue,
};

fn intervalo(inicio: u64, fin: u64) -> Intervalo {
    Intervalo::nuevo(inicio, fin).expect("inicio ≤ fin")
}

/// Configuraciones al azar: inicios no decrecientes (con repetidos), largos
/// variados, pliegues y embargo distintos.
fn configuraciones() -> Vec<(Vec<Intervalo>, usize, u64)> {
    let mut g = Generador::nuevo(42);
    (0..300)
        .map(|_| {
            let n = 4 + g.entero_bajo(200) as usize;
            let mut inicio = g.entero_bajo(5);
            let intervalos = (0..n)
                .map(|_| {
                    inicio += g.entero_bajo(3); // a veces repite: inicios iguales
                    intervalo(inicio, inicio + g.entero_bajo(15))
                })
                .collect();
            let pliegues = 2 + g.entero_bajo((n as u64 - 1).min(9)) as usize;
            let embargo = g.entero_bajo(25);
            (intervalos, pliegues, embargo)
        })
        .collect()
}

fn fin_de_la_validacion(intervalos: &[Intervalo], pliegue: &Pliegue) -> u64 {
    pliegue
        .validacion
        .iter()
        .map(|&i| intervalos[i].fin())
        .max()
        .expect("validación no vacía")
}

#[test]
fn ejemplo_a_mano() {
    // Diez observaciones; cada una depende de su instante y de los dos siguientes.
    let intervalos: Vec<Intervalo> = (0..10).map(|t| intervalo(t, t + 2)).collect();
    let pliegues = particionar(&intervalos, 2, 1).expect("partición válida");

    // Validación 0..5 abarca [0, 6]: 5 ([5,7]) y 6 ([6,8]) se cruzan; 7 ([7,9])
    // empieza justo después del fin y cae en el embargo de 1.
    assert_eq!(
        pliegues[0],
        Pliegue {
            validacion: vec![0, 1, 2, 3, 4],
            entrenamiento: vec![8, 9],
            purgadas: 2,
            embargadas: 1
        }
    );
    // Validación 5..10 abarca [5, 11]: 3 ([3,5]) y 4 ([4,6]) se cruzan; detrás
    // no queda nada que embargar.
    assert_eq!(
        pliegues[1],
        Pliegue {
            validacion: vec![5, 6, 7, 8, 9],
            entrenamiento: vec![0, 1, 2],
            purgadas: 2,
            embargadas: 0
        }
    );
}

#[test]
fn ninguna_observacion_de_entrenamiento_comparte_informacion_con_su_validacion() {
    for (intervalos, k, embargo) in configuraciones() {
        for pliegue in particionar(&intervalos, k, embargo).expect("partición válida") {
            for &j in &pliegue.entrenamiento {
                for &i in &pliegue.validacion {
                    assert!(
                        !intervalos[j].se_cruza_con(&intervalos[i]),
                        "entrenamiento {j} {:?} se cruza con validación {i} {:?}",
                        intervalos[j],
                        intervalos[i]
                    );
                }
            }
        }
    }
}

#[test]
fn el_embargo_aparta_lo_que_sigue_a_la_validacion() {
    let mut embargadas_en_total = 0;
    for (intervalos, k, embargo) in configuraciones() {
        for pliegue in particionar(&intervalos, k, embargo).expect("partición válida") {
            let fin = fin_de_la_validacion(&intervalos, &pliegue);
            for &j in &pliegue.entrenamiento {
                let inicio = intervalos[j].inicio();
                assert!(
                    !(inicio > fin && inicio <= fin + embargo),
                    "entrenamiento {j} empieza en {inicio}, dentro del embargo ({fin}, {}]",
                    fin + embargo
                );
            }
            embargadas_en_total += pliegue.embargadas;
        }
    }
    // Las configuraciones ejercitan de verdad el embargo.
    assert!(
        embargadas_en_total > 1000,
        "solo {embargadas_en_total} embargadas"
    );
}

#[test]
fn cada_observacion_se_valida_exactamente_una_vez() {
    for (intervalos, k, embargo) in configuraciones() {
        let pliegues = particionar(&intervalos, k, embargo).expect("partición válida");
        assert_eq!(pliegues.len(), k);
        let mut validadas: Vec<usize> =
            pliegues.iter().flat_map(|p| p.validacion.clone()).collect();
        validadas.sort_unstable();
        assert_eq!(validadas, (0..intervalos.len()).collect::<Vec<_>>());
        for p in &pliegues {
            // Lo que no se valida se entrena, se purga o se embarga: no se pierde nada.
            assert_eq!(
                p.validacion.len() + p.entrenamiento.len() + p.purgadas + p.embargadas,
                intervalos.len()
            );
            assert!(p.entrenamiento.iter().all(|j| !p.validacion.contains(j)));
            assert!(
                p.entrenamiento.windows(2).all(|w| w[0] < w[1]),
                "entrenamiento ordenado"
            );
        }
    }
}

/// La purga no es excesiva: lo que queda fuera del entrenamiento sin estar
/// en la validación se cruza con lo que abarca el bloque o está en el embargo.
#[test]
fn solo_se_aparta_lo_que_hace_falta() {
    for (intervalos, k, embargo) in configuraciones() {
        for pliegue in particionar(&intervalos, k, embargo).expect("partición válida") {
            let inicio = pliegue
                .validacion
                .iter()
                .map(|&i| intervalos[i].inicio())
                .min()
                .expect("no vacía");
            let fin = fin_de_la_validacion(&intervalos, &pliegue);
            let abarcado = intervalo(inicio, fin);
            for (j, apartado) in intervalos.iter().enumerate() {
                if pliegue.validacion.contains(&j) || pliegue.entrenamiento.contains(&j) {
                    continue;
                }
                let empieza = apartado.inicio();
                assert!(
                    apartado.se_cruza_con(&abarcado) || (empieza > fin && empieza <= fin + embargo),
                    "{j} se apartó sin motivo"
                );
            }
        }
    }
}

#[test]
fn intervalos() {
    assert_eq!(Intervalo::nuevo(5, 4), None);
    let a = intervalo(0, 2);
    // Compartir un extremo es compartir un instante.
    assert!(a.se_cruza_con(&intervalo(2, 4)) && intervalo(2, 4).se_cruza_con(&a));
    assert!(!a.se_cruza_con(&intervalo(3, 4)) && !intervalo(3, 4).se_cruza_con(&a));
    assert!(a.se_cruza_con(&intervalo(1, 1)));
}

#[test]
fn errores() {
    let intervalos: Vec<Intervalo> = (0..5).map(|t| intervalo(t, t)).collect();
    assert_eq!(
        particionar(&intervalos, 1, 0),
        Err(ErrorParticion::PocosPliegues { pliegues: 1 })
    );
    assert_eq!(
        particionar(&intervalos, 6, 0),
        Err(ErrorParticion::MasPlieguesQueObservaciones {
            pliegues: 6,
            observaciones: 5
        })
    );
    let desordenados = [intervalo(0, 1), intervalo(3, 4), intervalo(2, 5)];
    assert_eq!(
        particionar(&desordenados, 2, 0),
        Err(ErrorParticion::FueraDeOrden { indice: 2 })
    );
    // Un embargo enorme no desborda.
    assert!(particionar(&intervalos, 2, u64::MAX).is_ok());
}

#[test]
fn el_kfold_barajado_reparte_todo_y_depende_de_la_semilla() {
    let pliegues = kfold_barajado(103, 5, &mut Generador::nuevo(1)).expect("válido");
    let mut validadas: Vec<usize> = pliegues.iter().flat_map(|p| p.validacion.clone()).collect();
    validadas.sort_unstable();
    assert_eq!(validadas, (0..103).collect::<Vec<_>>());
    for p in &pliegues {
        assert_eq!(p.validacion.len() + p.entrenamiento.len(), 103);
    }
    assert_eq!(
        pliegues,
        kfold_barajado(103, 5, &mut Generador::nuevo(1)).expect("válido")
    );
    assert_ne!(
        pliegues,
        kfold_barajado(103, 5, &mut Generador::nuevo(2)).expect("válido")
    );
}
