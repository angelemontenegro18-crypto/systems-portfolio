//! La evaluación: barrido exhaustivo de la tabla de la prensa, diferencial contra el evaluador
//! ingenuo en tablas al azar con semillas fijas, y el cierre en falla.

mod comun;

use comun::{evaluar, referencia, tabla_valida, Azar};
use policy_kernel::orden::MoverValvula;
use policy_kernel::politica::{Decision, Motivo, Politica, PRENSA, REGLAS_DE_LA_PRENSA};
use policy_kernel::regla::Clase;

#[test]
fn barrido_exhaustivo_de_la_prensa_contra_la_referencia() {
    for clase in Clase::TODAS {
        for valor in 0..=u8::MAX {
            assert_eq!(
                evaluar(&PRENSA, clase, valor),
                referencia(&REGLAS_DE_LA_PRENSA, clase, i32::from(valor)),
                "{clase:?} = {valor}"
            );
        }
    }
}

#[test]
fn los_bordes_de_la_prensa() {
    let fuera = Err(Motivo::FueraDeRango);
    for (clase, valor, esperado) in [
        (Clase::Apertura, 0, Ok(())),
        (Clase::Apertura, 80, Ok(())),
        (Clase::Apertura, 81, fuera),
        (Clase::Apertura, 89, fuera),
        (Clase::Apertura, 90, Ok(())),
        (Clase::Apertura, 100, Ok(())),
        (Clase::Apertura, 101, fuera),
        (Clase::Apertura, 255, fuera),
        (Clase::Presion, 160, Ok(())),
        (Clase::Presion, 161, fuera),
        (Clase::Avance, 25, Ok(())),
        (Clase::Avance, 26, fuera),
        (Clase::Purga, 0, Err(Motivo::SinRegla)),
        (Clase::Purga, 30, Err(Motivo::SinRegla)),
    ] {
        assert_eq!(
            evaluar(&PRENSA, clase, valor),
            esperado,
            "{clase:?} = {valor}"
        );
    }
}

#[test]
fn cuantas_ordenes_permite_la_prensa() {
    // De las 4 × 256 órdenes posibles: 81 + 11 aperturas, 161 presiones, 26 avances, ninguna
    // purga.
    let permitidas = Clase::TODAS
        .iter()
        .flat_map(|&c| (0..=u8::MAX).map(move |v| (c, v)))
        .filter(|&(c, v)| evaluar(&PRENSA, c, v).is_ok())
        .count();
    assert_eq!(permitidas, 81 + 11 + 161 + 26);
}

#[test]
fn diferencial_contra_la_referencia_en_tablas_al_azar() {
    let mut evaluaciones = 0;
    for semilla in 1..=500u64 {
        let tabla = tabla_valida(&mut Azar(semilla));
        let politica = Politica::con_reglas(&tabla).expect("válida por construcción");
        for clase in Clase::TODAS {
            for valor in 0..=u8::MAX {
                assert_eq!(
                    evaluar(&politica, clase, valor),
                    referencia(&tabla, clase, i32::from(valor)),
                    "semilla {semilla}, {clase:?} = {valor}: {tabla:?}"
                );
                evaluaciones += 1;
            }
        }
    }
    assert_eq!(evaluaciones, 500 * 4 * 256);
}

#[test]
fn con_la_tabla_vacia_todo_se_deniega() {
    let nada = Politica::con_reglas(&[]).expect("la tabla vacía es válida");
    for clase in Clase::TODAS {
        for valor in [0, 1, 100, 255] {
            assert_eq!(evaluar(&nada, clase, valor), Err(Motivo::SinRegla));
        }
    }
}

#[test]
fn la_autorizacion_lleva_la_orden_que_se_evaluo() {
    match PRENSA.evaluar(MoverValvula { apertura: 42 }) {
        Decision::Permitir(autorizada) => {
            assert_eq!(*autorizada.orden(), MoverValvula { apertura: 42 })
        }
        Decision::Denegar(motivo) => panic!("denegada: {motivo:?}"),
    }
}
