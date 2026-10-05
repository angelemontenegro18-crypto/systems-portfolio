//! La envolvente no se amplía, y lo que queda fuera nunca toca el actuador.

mod comun;

use autonomous_remediation::{
    Accion, Desenlace, Envolvente, Permiso, Propuesta, Puerta, Remediador,
};
use comun::llegadas::Generador;
use comun::{metricas, politica, propuesta, servicio, ActuadorDePrueba, Llamada};

/// Propuestas que tienen que quedar fuera siempre.
fn siempre_afuera() -> Vec<Propuesta> {
    vec![
        // Una réplica más que el máximo permitido.
        propuesta(
            Accion::EscalarReplicas {
                servicio: servicio("api"),
                actuales: 6,
                nuevas: 7,
            },
            Some(0.99),
            Some(metricas(0.0, 2.0, 0.5, 6)),
        ),
        // Un servicio sin permisos.
        propuesta(
            Accion::Reiniciar {
                servicio: servicio("pagos"),
            },
            Some(0.99),
            Some(metricas(0.5, 0.6, 0.5, 4)),
        ),
        // Una acción que `catalogo` no tiene permitida.
        propuesta(
            Accion::Reiniciar {
                servicio: servicio("catalogo"),
            },
            Some(0.99),
            Some(metricas(0.5, 0.6, 0.5, 4)),
        ),
    ]
}

/// Una propuesta al azar: cualquier acción sobre cualquier servicio, con datos
/// a veces buenos, a veces malos y a veces ausentes.
fn propuesta_al_azar(g: &mut Generador) -> Propuesta {
    let nombres = ["api", "catalogo", "pagos"];
    let s = servicio(nombres[(g.siguiente() % 3) as usize]);
    let actuales = 1 + (g.siguiente() % 8) as u32;
    let accion = match g.siguiente() % 3 {
        0 => Accion::Reiniciar { servicio: s },
        1 => Accion::EscalarReplicas {
            servicio: s,
            actuales,
            nuevas: 1 + (g.siguiente() % 9) as u32,
        },
        _ => Accion::VaciarCache { servicio: s },
    };
    let confianza = (!g.siguiente().is_multiple_of(5)).then(|| g.uniforme());
    let replicas = if g.siguiente().is_multiple_of(4) {
        actuales + 1
    } else {
        actuales
    };
    let datos = metricas(
        g.uniforme() * 0.5,
        g.uniforme() * 2.0,
        g.uniforme(),
        replicas,
    );
    let metricas = (!g.siguiente().is_multiple_of(5)).then_some(datos);
    propuesta(accion, confianza, metricas)
}

/// El trinquete: después de un éxito en el borde del rango y de cientos de
/// propuestas al azar —éxitos, rechazos y fallas del actuador—, la envolvente
/// es exactamente la del principio y lo que estaba afuera sigue afuera.
#[test]
fn la_envolvente_no_se_amplia_con_el_uso() {
    for fallas in [vec![], vec![Llamada::Aplicar], vec![Llamada::Comprobar]] {
        let original = politica();
        let mut remediador =
            Remediador::nuevo(original.clone(), ActuadorDePrueba::que_falla_en(&fallas));

        // Un éxito justo en el máximo: escalar de 3 a 6.
        let borde = propuesta(
            Accion::EscalarReplicas {
                servicio: servicio("api"),
                actuales: 3,
                nuevas: 6,
            },
            Some(0.97),
            Some(metricas(0.0, 1.4, 0.5, 3)),
        );
        let desenlace = remediador.atender(&borde);
        if fallas.is_empty() {
            assert!(
                matches!(desenlace, Desenlace::Autoaplicada { .. }),
                "{desenlace:?}"
            );
        }

        let mut g = Generador::nuevo(31);
        for _ in 0..600 {
            let p = propuesta_al_azar(&mut g);
            let antes = remediador.actuador().llamadas.len();
            let autorizada = remediador
                .politica()
                .evaluar(&p)
                .veredicto(Puerta::Autoridad)
                .pasa();
            remediador.atender(&p);
            if !autorizada {
                assert_eq!(
                    remediador.actuador().llamadas.len(),
                    antes,
                    "{} tocó el actuador",
                    p.accion
                );
            }
            assert_eq!(
                remediador.politica(),
                &original,
                "la política cambió después de {}",
                p.accion
            );
        }
        for ajena in siempre_afuera() {
            let antes = remediador.actuador().llamadas.len();
            let Desenlace::Ticket(t) = remediador.atender(&ajena) else {
                panic!("{} quedó autorizada", ajena.accion);
            };
            assert!(
                t.puertas_fallidas().contains(&Puerta::Autoridad),
                "{}",
                ajena.accion
            );
            assert_eq!(remediador.actuador().llamadas.len(), antes);
        }
    }
}

#[test]
fn los_permisos_de_escalar_tienen_un_rango_valido() {
    let api = servicio("api");
    assert!(
        Permiso::escalar(api.clone(), 0, 3).is_none(),
        "el cero no es una remediación"
    );
    assert!(Permiso::escalar(api.clone(), 4, 3).is_none(), "rango vacío");
    assert!(Permiso::escalar(api, 2, 2).is_some());
}

#[test]
fn el_rango_incluye_sus_extremos() {
    let envolvente = politica().envolvente().clone();
    let escalar = |nuevas| Accion::EscalarReplicas {
        servicio: servicio("api"),
        actuales: 3,
        nuevas,
    };
    assert!(envolvente.admite(&escalar(2)).is_ok());
    assert!(envolvente.admite(&escalar(6)).is_ok());
    assert!(envolvente.admite(&escalar(1)).is_err());
    assert!(envolvente.admite(&escalar(7)).is_err());
}

#[test]
fn una_envolvente_vacia_no_admite_nada() {
    let vacia = Envolvente::vacia();
    for accion in [
        Accion::Reiniciar {
            servicio: servicio("api"),
        },
        Accion::VaciarCache {
            servicio: servicio("api"),
        },
        Accion::EscalarReplicas {
            servicio: servicio("api"),
            actuales: 2,
            nuevas: 3,
        },
    ] {
        assert!(vacia.admite(&accion).is_err(), "{accion}");
    }
}
