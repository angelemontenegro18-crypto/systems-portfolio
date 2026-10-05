//! El orden: la vuelta atrás se arma y se verifica antes de aplicar, y se usa
//! si aplicar o comprobar fallan.

mod comun;

use autonomous_remediation::{Desenlace, Estado, Remediador};
use comun::{escalar_sano, politica, reiniciar_sano, vaciar_sano, ActuadorDePrueba, Llamada};

fn atender_con_fallas(fallas: &[Llamada]) -> (Desenlace, Vec<Llamada>) {
    let mut remediador = Remediador::nuevo(politica(), ActuadorDePrueba::que_falla_en(fallas));
    let desenlace = remediador.atender(&escalar_sano());
    (desenlace, remediador.actuador().llamadas.clone())
}

#[test]
fn la_vuelta_atras_se_arma_y_se_verifica_antes_de_aplicar() {
    for p in [reiniciar_sano(), escalar_sano(), vaciar_sano()] {
        let mut remediador = Remediador::nuevo(politica(), ActuadorDePrueba::default());
        assert!(matches!(
            remediador.atender(&p),
            Desenlace::Autoaplicada { .. }
        ));
        assert_eq!(
            remediador.actuador().llamadas,
            [
                Llamada::Armar,
                Llamada::Verificar,
                Llamada::Aplicar,
                Llamada::Comprobar
            ],
            "{}",
            p.accion
        );
    }
}

#[test]
fn si_aplicar_falla_se_revierte() {
    let (desenlace, llamadas) = atender_con_fallas(&[Llamada::Aplicar]);
    assert_eq!(
        llamadas,
        [
            Llamada::Armar,
            Llamada::Verificar,
            Llamada::Aplicar,
            Llamada::Revertir
        ]
    );
    let Desenlace::Ticket(t) = desenlace else {
        panic!("se esperaba un ticket")
    };
    assert!(
        matches!(t.estado(), Estado::Revertida { fallo } if fallo.contains("la aplicación falló")),
        "{:?}",
        t.estado()
    );
    assert!(!t.requiere_atencion_inmediata());
    // Las puertas habían pasado: lo que falló fue la ejecución, y el ticket lo dice.
    assert!(t.evaluacion().autoaplicable());
    assert!(t.to_string().contains("se aplicó y se revirtió"));
}

#[test]
fn si_la_comprobacion_posterior_falla_se_revierte() {
    let (desenlace, llamadas) = atender_con_fallas(&[Llamada::Comprobar]);
    assert_eq!(
        llamadas,
        [
            Llamada::Armar,
            Llamada::Verificar,
            Llamada::Aplicar,
            Llamada::Comprobar,
            Llamada::Revertir
        ]
    );
    let Desenlace::Ticket(t) = desenlace else {
        panic!("se esperaba un ticket")
    };
    assert!(
        matches!(t.estado(), Estado::Revertida { fallo } if fallo.contains("comprobación")),
        "{:?}",
        t.estado()
    );
}

#[test]
fn si_la_vuelta_atras_tambien_falla_el_ticket_es_urgente() {
    let (desenlace, llamadas) = atender_con_fallas(&[Llamada::Aplicar, Llamada::Revertir]);
    assert_eq!(
        llamadas,
        [
            Llamada::Armar,
            Llamada::Verificar,
            Llamada::Aplicar,
            Llamada::Revertir
        ]
    );
    let Desenlace::Ticket(t) = desenlace else {
        panic!("se esperaba un ticket")
    };
    assert!(
        matches!(t.estado(), Estado::ReversionFallida { .. }),
        "{:?}",
        t.estado()
    );
    assert!(t.requiere_atencion_inmediata());
    let texto = t.to_string();
    assert!(texto.starts_with("TICKET URGENTE"), "{texto}");
    assert!(texto.contains("NO se pudo revertir"), "{texto}");
}

/// Evaluar no cambia nada: la misma propuesta da la misma evaluación, y la
/// política queda igual.
#[test]
fn evaluar_es_una_funcion_pura() {
    let p = politica();
    let antes = p.clone();
    for propuesta in [reiniciar_sano(), escalar_sano(), vaciar_sano()] {
        assert_eq!(p.evaluar(&propuesta), p.evaluar(&propuesta));
    }
    assert_eq!(p, antes);
}
