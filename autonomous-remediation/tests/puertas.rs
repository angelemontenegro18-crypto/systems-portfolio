//! Las cuatro puertas: cada una, sola, bloquea; solo las cuatro juntas
//! permiten autoaplicar.

mod comun;

use autonomous_remediation::{Accion, Desenlace, Estado, Propuesta, Puerta, Remediador, Ticket};
use comun::{
    escalar_sano, metricas, politica, propuesta, reiniciar_sano, servicio, vaciar_sano,
    ActuadorDePrueba, Llamada,
};

/// Atiende la propuesta con un actuador de prueba y devuelve el desenlace y
/// las llamadas que recibió.
fn atender_con(propuesta: &Propuesta, actuador: ActuadorDePrueba) -> (Desenlace, Vec<Llamada>) {
    let mut remediador = Remediador::nuevo(politica(), actuador);
    let desenlace = remediador.atender(propuesta);
    (desenlace, remediador.actuador().llamadas.clone())
}

fn atender(propuesta: &Propuesta) -> (Desenlace, Vec<Llamada>) {
    atender_con(propuesta, ActuadorDePrueba::default())
}

fn ticket(desenlace: Desenlace) -> Ticket {
    match desenlace {
        Desenlace::Ticket(t) => t,
        Desenlace::Autoaplicada { accion, .. } => {
            panic!("se autoaplicó {accion}, se esperaba un ticket")
        }
    }
}

fn escalar(actuales: u32, nuevas: u32, utilizacion: f64, confianza: f64) -> Propuesta {
    propuesta(
        Accion::EscalarReplicas {
            servicio: servicio("api"),
            actuales,
            nuevas,
        },
        Some(confianza),
        Some(metricas(0.0, utilizacion, 0.5, actuales)),
    )
}

#[test]
fn las_cuatro_puertas_juntas_autoaplican() {
    for p in [reiniciar_sano(), escalar_sano(), vaciar_sano()] {
        let (desenlace, llamadas) = atender(&p);
        let Desenlace::Autoaplicada { accion, evaluacion } = desenlace else {
            panic!("{} debería autoaplicarse: {desenlace:?}", p.accion);
        };
        assert_eq!(accion, p.accion);
        for puerta in Puerta::TODAS {
            assert!(
                evaluacion.veredicto(puerta).pasa(),
                "{puerta}: {:?}",
                evaluacion.veredicto(puerta)
            );
        }
        assert_eq!(
            llamadas,
            [
                Llamada::Armar,
                Llamada::Verificar,
                Llamada::Aplicar,
                Llamada::Comprobar
            ]
        );
    }
}

#[test]
fn cada_puerta_sola_bloquea_el_autoaplicar() {
    let catalogo_cargado = propuesta(
        Accion::VaciarCache {
            servicio: servicio("catalogo"),
        },
        Some(0.97),
        Some(metricas(0.20, 0.7, 0.9, 4)),
    );
    let mut poca_confianza = reiniciar_sano();
    poca_confianza.confianza = Some(0.62);
    let casos = [
        (
            "autoridad: 8 réplicas, fuera de [2, 6]",
            escalar(3, 8, 1.4, 0.97),
            Puerta::Autoridad,
        ),
        (
            "reversibilidad: volver a 1 réplica, fuera de [2, 6]",
            escalar(1, 3, 1.4, 0.97),
            Puerta::Reversibilidad,
        ),
        (
            "beneficio neto: daño 0.9 × 0.7 = 0.63 > 0.30",
            catalogo_cargado,
            Puerta::BeneficioNeto,
        ),
        ("confianza: 0.62 < 0.90", poca_confianza, Puerta::Confianza),
    ];
    for (caso, p, puerta) in casos {
        let (desenlace, llamadas) = atender(&p);
        let t = ticket(desenlace);
        assert_eq!(t.puertas_fallidas(), [puerta], "{caso}");
        assert_eq!(*t.estado(), Estado::NoSeAplico, "{caso}");
        assert!(
            llamadas.is_empty(),
            "{caso}: una puerta que falla no toca el actuador, y hubo {llamadas:?}"
        );
    }
}

/// Las dieciséis combinaciones de cuatro puertas que pasan o fallan. Solo la
/// que pasa las cuatro se autoaplica, y el ticket nombra exactamente las que
/// fallaron. Cada falla se provoca sin tocar las otras puertas:
///
/// - autoridad: 7 réplicas en vez de 5 (fuera de `[2, 6]`);
/// - reversibilidad: partir de 1 réplica en vez de 3 (volver a 1, fuera de `[2, 6]`);
/// - beneficio neto: utilización 0.5 en vez de 1.4 (sin rechazo que evitar);
/// - confianza: 0.5 en vez de 0.97.
#[test]
fn de_las_dieciseis_combinaciones_solo_autoaplica_la_que_pasa_las_cuatro() {
    for combinacion in 0u8..16 {
        let falla = |bit: u8| combinacion & (1 << bit) != 0;
        let p = escalar(
            if falla(1) { 1 } else { 3 },
            if falla(0) { 7 } else { 5 },
            if falla(2) { 0.5 } else { 1.4 },
            if falla(3) { 0.5 } else { 0.97 },
        );
        let esperadas: Vec<Puerta> = Puerta::TODAS
            .into_iter()
            .enumerate()
            .filter(|(i, _)| falla(*i as u8))
            .map(|(_, p)| p)
            .collect();

        let (desenlace, llamadas) = atender(&p);
        match desenlace {
            Desenlace::Autoaplicada { .. } => assert!(
                esperadas.is_empty(),
                "{combinacion:04b}: se autoaplicó con {esperadas:?} en falla"
            ),
            Desenlace::Ticket(t) => {
                assert_eq!(t.puertas_fallidas(), esperadas, "{combinacion:04b}");
                assert!(llamadas.is_empty(), "{combinacion:04b}: {llamadas:?}");
            }
        }
    }
}

/// Confianza 1.0 y el mayor beneficio posible no compensan un daño apenas por
/// encima del tope.
#[test]
fn una_puerta_holgada_no_compensa_otra_que_falla() {
    let p = propuesta(
        Accion::Reiniciar {
            servicio: servicio("api"),
        },
        Some(1.0),
        Some(metricas(1.0, 0.6, 0.5, 3)), // daño 1/3 = 0.33 > 0.30
    );
    let t = ticket(atender(&p).0);
    assert_eq!(t.puertas_fallidas(), [Puerta::BeneficioNeto]);
    assert!(t
        .evaluacion()
        .veredicto(Puerta::BeneficioNeto)
        .detalle()
        .contains("0.33 > tope 0.30"));
}

#[test]
fn un_dato_ausente_o_invalido_cierra_su_puerta() {
    let con = |confianza: Option<f64>| {
        let mut p = reiniciar_sano();
        p.confianza = confianza;
        p
    };
    let sin_metricas = {
        let mut p = reiniciar_sano();
        p.metricas = None;
        p
    };
    let con_metricas = |tasa: f64, util: f64, aciertos: f64, replicas: u32| {
        let mut p = reiniciar_sano();
        p.metricas = Some(metricas(tasa, util, aciertos, replicas));
        p
    };
    let replicas_inconsistentes = propuesta(
        Accion::EscalarReplicas {
            servicio: servicio("api"),
            actuales: 3,
            nuevas: 5,
        },
        Some(0.97),
        Some(metricas(0.0, 1.4, 0.5, 4)),
    );
    let casos = [
        ("sin confianza", con(None), Puerta::Confianza, "sin dato"),
        (
            "confianza NaN",
            con(Some(f64::NAN)),
            Puerta::Confianza,
            "fuera de [0, 1]",
        ),
        (
            "confianza 1.5",
            con(Some(1.5)),
            Puerta::Confianza,
            "fuera de [0, 1]",
        ),
        (
            "sin métricas",
            sin_metricas,
            Puerta::BeneficioNeto,
            "sin métricas",
        ),
        (
            "tasa de error NaN",
            con_metricas(f64::NAN, 0.6, 0.5, 4),
            Puerta::BeneficioNeto,
            "tasa de error",
        ),
        (
            "utilización infinita",
            con_metricas(0.3, f64::INFINITY, 0.5, 4),
            Puerta::BeneficioNeto,
            "utilización",
        ),
        (
            "aciertos negativos",
            con_metricas(0.3, 0.6, -0.1, 4),
            Puerta::BeneficioNeto,
            "aciertos",
        ),
        (
            "sin réplicas sanas",
            con_metricas(0.3, 0.6, 0.5, 0),
            Puerta::BeneficioNeto,
            "sin réplicas",
        ),
        (
            "réplicas que no coinciden",
            replicas_inconsistentes,
            Puerta::BeneficioNeto,
            "dicen 4",
        ),
    ];
    for (caso, p, puerta, explicacion) in casos {
        let (desenlace, llamadas) = atender(&p);
        let t = ticket(desenlace);
        assert_eq!(t.puertas_fallidas(), [puerta], "{caso}");
        let detalle = t.evaluacion().veredicto(puerta).detalle();
        assert!(detalle.contains(explicacion), "{caso}: {detalle:?}");
        assert!(llamadas.is_empty(), "{caso}: {llamadas:?}");
    }
}

/// Las cuatro fallan a la vez: el ticket nombra cada una con sus valores, y
/// dice qué se iba a hacer, por qué y cómo revertir.
#[test]
fn cada_puerta_que_falla_aparece_nombrada_en_el_ticket_con_sus_valores() {
    let mut p = escalar(1, 7, 0.5, 0.5);
    p.motivo = "utilización sostenida por encima de 1".to_string();
    let t = ticket(atender(&p).0);
    assert_eq!(t.puertas_fallidas(), Puerta::TODAS);

    let texto = t.to_string();
    for esperado in [
        "TICKET · escalar `api` de 1 a 7 réplicas",
        "Por qué:  utilización sostenida por encima de 1",
        "no se aplicó",
        "✗ autoridad",
        "escalar a 7 réplicas queda fuera de [2, 6]",
        "✗ reversibilidad",
        "volver a 1 réplica queda fuera de [2, 6]",
        "✗ beneficio neto",
        "beneficio 0.00, no mayor que 0",
        "✗ confianza",
        "0.50 < mínimo 0.90",
        "Cómo revertir: escalar `api` de vuelta a 1 réplica",
    ] {
        assert!(texto.contains(esperado), "falta {esperado:?} en:\n{texto}");
    }
}

/// Con las otras tres en verde, la reversibilidad que no se arma o no se
/// verifica también bloquea.
#[test]
fn una_vuelta_atras_que_no_se_arma_o_no_se_verifica_bloquea() {
    for (falla, esperadas, explicacion) in [
        (Llamada::Armar, vec![Llamada::Armar], "no se pudo armar"),
        (
            Llamada::Verificar,
            vec![Llamada::Armar, Llamada::Verificar],
            "no pasó la verificación",
        ),
    ] {
        let (desenlace, llamadas) =
            atender_con(&escalar_sano(), ActuadorDePrueba::que_falla_en(&[falla]));
        let t = ticket(desenlace);
        assert_eq!(t.puertas_fallidas(), [Puerta::Reversibilidad], "{falla:?}");
        assert!(t
            .evaluacion()
            .veredicto(Puerta::Reversibilidad)
            .detalle()
            .contains(explicacion));
        assert_eq!(llamadas, esperadas, "nunca se aplica");
    }
}
