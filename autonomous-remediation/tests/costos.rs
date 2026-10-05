//! El modelo de costos ilustrativo: sus valores a mano y sus rechazos.

mod comun;

use autonomous_remediation::costos::simular;
use autonomous_remediation::Accion;
use comun::{metricas, servicio};

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn reiniciar() {
    let s = simular(
        &Accion::Reiniciar {
            servicio: servicio("api"),
        },
        &metricas(0.3, 0.6, 0.5, 4),
    )
    .expect("válido");
    assert!(cerca(s.beneficio, 0.3) && cerca(s.dano, 0.25), "{s:?}");
}

#[test]
fn escalar() {
    let escalar = |actuales, nuevas| Accion::EscalarReplicas {
        servicio: servicio("api"),
        actuales,
        nuevas,
    };
    // Utilización 1.4: se rechaza 1 − 1/1.4 de las solicitudes. Con 5 réplicas
    // en vez de 3, la utilización baja a 0.84 y ya no se rechaza nada.
    let s = simular(&escalar(3, 5), &metricas(0.0, 1.4, 0.5, 3)).expect("válido");
    assert!(
        cerca(s.beneficio, 1.0 - 1.0 / 1.4) && s.dano == 0.0,
        "{s:?}"
    );
    // Bajar de 5 a 3 con utilización 0.9: pasa a 1.5 y se rechaza 1/3.
    let s = simular(&escalar(5, 3), &metricas(0.0, 0.9, 0.5, 5)).expect("válido");
    assert!(
        s.beneficio == 0.0 && cerca(s.dano, 1.0 - 1.0 / 1.5),
        "{s:?}"
    );
    // Sin sobrecarga, escalar no aporta.
    let s = simular(&escalar(3, 5), &metricas(0.0, 0.5, 0.5, 3)).expect("válido");
    assert!(s.beneficio == 0.0 && s.dano == 0.0, "{s:?}");
}

#[test]
fn vaciar_la_cache() {
    let vaciar = Accion::VaciarCache {
        servicio: servicio("api"),
    };
    let s = simular(&vaciar, &metricas(0.2, 0.5, 0.8, 4)).expect("válido");
    assert!(cerca(s.beneficio, 0.2) && cerca(s.dano, 0.4), "{s:?}");
    // Por encima de la capacidad, el daño no pasa de los aciertos.
    let s = simular(&vaciar, &metricas(0.2, 1.3, 0.8, 4)).expect("válido");
    assert!(cerca(s.dano, 0.8), "{s:?}");
}

#[test]
fn los_datos_invalidos_o_inconsistentes_se_rechazan() {
    let reiniciar = Accion::Reiniciar {
        servicio: servicio("api"),
    };
    for m in [
        metricas(f64::NAN, 0.6, 0.5, 4),
        metricas(1.2, 0.6, 0.5, 4),
        metricas(0.3, -0.1, 0.5, 4),
        metricas(0.3, f64::INFINITY, 0.5, 4),
        metricas(0.3, f64::NAN, 0.5, 4),
        metricas(0.3, 0.6, 1.5, 4),
        metricas(0.3, 0.6, 0.5, 0),
    ] {
        assert!(simular(&reiniciar, &m).is_err(), "{m:?}");
    }
    let escalar = |actuales, nuevas| Accion::EscalarReplicas {
        servicio: servicio("api"),
        actuales,
        nuevas,
    };
    assert!(
        simular(&escalar(3, 5), &metricas(0.0, 1.4, 0.5, 4)).is_err(),
        "réplicas que no coinciden"
    );
    assert!(
        simular(&escalar(3, 0), &metricas(0.0, 1.4, 0.5, 3)).is_err(),
        "a cero réplicas"
    );
}
