//! La validación de tablas: contra un validador de referencia en tablas al azar, y en los
//! casos de borde (rangos que se tocan en un valor, vacíos, desordenados).

mod comun;

use comun::{tabla_cualquiera, tabla_valida, validar_referencia, Azar};
use policy_kernel::politica::{Politica, REGLAS_DE_LA_PRENSA};
use policy_kernel::regla::{validar, Clase, Rango, Regla};

#[test]
fn la_tabla_de_la_prensa_es_valida() {
    assert!(validar(&REGLAS_DE_LA_PRENSA));
    assert!(validar_referencia(&REGLAS_DE_LA_PRENSA));
    assert!(Politica::con_reglas(&REGLAS_DE_LA_PRENSA).is_some());
}

#[test]
fn validar_coincide_con_la_referencia_en_tablas_al_azar() {
    let (mut validas, mut invalidas) = (0, 0);
    for semilla in 1..=4000u64 {
        let mut azar = Azar(semilla);
        let tabla = tabla_cualquiera(&mut azar);
        let esperado = validar_referencia(&tabla);
        assert_eq!(validar(&tabla), esperado, "semilla {semilla}: {tabla:?}");
        assert_eq!(Politica::con_reglas(&tabla).is_some(), esperado);
        if esperado {
            validas += 1;
        } else {
            invalidas += 1;
        }
    }
    // Las tablas al azar ejercitan los dos lados.
    println!("{validas} válidas y {invalidas} inválidas");
    assert!(validas > 500 && invalidas > 500);
}

#[test]
fn las_tablas_validas_por_construccion_pasan() {
    for semilla in 1..=1000u64 {
        let tabla = tabla_valida(&mut Azar(semilla));
        assert!(validar(&tabla), "semilla {semilla}: {tabla:?}");
        assert!(validar_referencia(&tabla));
    }
}

#[test]
fn dos_rangos_que_comparten_un_solo_valor_se_solapan() {
    let a = Regla::nueva(Clase::Presion, 0, 50);
    assert!(!validar(&[a, Regla::nueva(Clase::Presion, 50, 80)]));
    assert!(validar(&[a, Regla::nueva(Clase::Presion, 51, 80)]));
    // En clases distintas no hay solapamiento posible.
    assert!(validar(&[a, Regla::nueva(Clase::Avance, 0, 50)]));
}

#[test]
fn un_rango_vacio_invalida_la_tabla() {
    assert!(!validar(&[Regla::nueva(Clase::Apertura, 10, 9)]));
    assert!(validar(&[Regla::nueva(Clase::Apertura, 10, 10)]));
}

#[test]
fn una_tabla_desordenada_no_vale() {
    let presion = Regla::nueva(Clase::Presion, 0, 10);
    let apertura = Regla::nueva(Clase::Apertura, 0, 10);
    assert!(validar(&[apertura, presion]));
    assert!(!validar(&[presion, apertura]));
    let alta = Regla::nueva(Clase::Apertura, 50, 60);
    assert!(validar(&[apertura, alta]));
    assert!(!validar(&[alta, apertura]));
}

#[test]
fn la_tabla_vacia_es_valida() {
    assert!(validar(&[]));
}

#[test]
fn contiene_incluye_los_dos_extremos() {
    let r = Rango::<-3, 7>.regla(Clase::Avance);
    assert_eq!((r.clase(), r.min(), r.max()), (Clase::Avance, -3, 7));
    assert!(!r.contiene(-4));
    assert!(r.contiene(-3));
    assert!(r.contiene(7));
    assert!(!r.contiene(8));
    assert!(!r.contiene(i32::MIN) && !r.contiene(i32::MAX));
}
