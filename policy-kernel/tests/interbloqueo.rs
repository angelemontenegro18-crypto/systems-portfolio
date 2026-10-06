//! El interbloqueo: se arma solo con el resguardo cerrado, y armado aplica únicamente lo que
//! la política autorizó.

use policy_kernel::interbloqueo::Interbloqueo;
use policy_kernel::orden::{FijarAvance, FijarPresion, MoverValvula, Orden, Purgar};
use policy_kernel::politica::{Decision, PRENSA};
use policy_kernel::regla::Clase;

#[test]
fn con_el_resguardo_abierto_no_se_arma() {
    let desarmado = Interbloqueo::nuevo();
    let Err(sigue) = desarmado.armar(false) else {
        panic!("se armó con el resguardo abierto");
    };
    assert_eq!(sigue.aplicadas(), 0);
    assert!(sigue.armar(true).is_ok());
}

#[test]
fn armado_aplica_lo_autorizado_y_lo_cuenta() {
    let Ok(mut armado) = Interbloqueo::nuevo().armar(true) else {
        panic!("no se armó");
    };
    let Decision::Permitir(autorizada) = PRENSA.evaluar(FijarPresion { bar: 120 }) else {
        panic!("120 bar está permitido");
    };
    let comando = armado.aplicar(autorizada);
    assert_eq!((comando.clase(), comando.valor()), (Clase::Presion, 120));
    assert_eq!(armado.aplicadas(), 1);
    // Desarmarlo conserva la cuenta.
    let desarmado = armado.desarmar();
    assert_eq!(desarmado.aplicadas(), 1);
}

/// Pasa una orden por la política y, si se permite, por el interbloqueo armado.
fn al_hardware<O: Orden>(
    interbloqueo: &mut Interbloqueo<policy_kernel::interbloqueo::Armado>,
    orden: O,
) -> Option<(Clase, i32)> {
    match PRENSA.evaluar(orden) {
        Decision::Permitir(autorizada) => {
            let comando = interbloqueo.aplicar(autorizada);
            Some((comando.clase(), comando.valor()))
        }
        Decision::Denegar(_) => None,
    }
}

#[test]
fn una_orden_denegada_no_llega_al_hardware() {
    let Ok(mut armado) = Interbloqueo::nuevo().armar(true) else {
        panic!("no se armó");
    };
    let llegaron = [
        al_hardware(&mut armado, MoverValvula { apertura: 30 }),
        al_hardware(&mut armado, MoverValvula { apertura: 85 }),
        al_hardware(&mut armado, FijarPresion { bar: 200 }),
        al_hardware(&mut armado, FijarAvance { mm_por_s: 25 }),
        al_hardware(&mut armado, Purgar { segundos: 5 }),
        al_hardware(&mut armado, MoverValvula { apertura: 95 }),
    ];
    assert_eq!(
        llegaron,
        [
            Some((Clase::Apertura, 30)),
            None,
            None,
            Some((Clase::Avance, 25)),
            None,
            Some((Clase::Apertura, 95)),
        ]
    );
    assert_eq!(armado.aplicadas(), 3);
}
