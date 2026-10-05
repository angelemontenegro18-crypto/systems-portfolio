//! El formato sellado, sin disco.

use sealed_checkpoint::sello::{self, ErrorSello, LARGO_MINIMO};
use sealed_checkpoint::Clave;

fn clave(b: u8) -> Clave {
    Clave::desde_bytes([b; 32])
}

#[test]
fn ida_y_vuelta() {
    let s = sello::sellar(&clave(1), "sesion", 7, b"datos de prueba").expect("sellar");
    let a = sello::abrir(&clave(1), "sesion", &s).expect("abrir");
    assert_eq!(a.generacion, 7);
    assert_eq!(a.datos.as_slice(), b"datos de prueba");
}

#[test]
fn cualquier_bit_alterado_hace_fallar_la_apertura() {
    // Exhaustivo: cada bit de cada byte — encabezado, nonce, datos y etiqueta.
    let original = sello::sellar(&clave(1), "sesion", 42, b"estado").expect("sellar");
    let mut alterado = original.clone();
    for i in 0..original.len() {
        for bit in 0..8 {
            alterado[i] ^= 1 << bit;
            assert!(
                sello::abrir(&clave(1), "sesion", &alterado).is_err(),
                "el bit {bit} del byte {i} se alteró y el sello abrió igual"
            );
            alterado[i] ^= 1 << bit;
        }
    }
    assert_eq!(alterado, original, "el sello quedó como estaba");
}

#[test]
fn alterar_la_generacion_del_encabezado_no_pasa_desapercibido() {
    // La generación viaja en claro, pero autenticada: subirla a mano no funciona.
    let mut s = sello::sellar(&clave(1), "sesion", 3, b"x").expect("sellar");
    assert_eq!(sello::generacion_sin_verificar(&s), Ok(3));
    s[5] = 99;
    assert_eq!(
        sello::generacion_sin_verificar(&s),
        Ok(99),
        "se lee sin clave…"
    );
    assert!(
        matches!(
            sello::abrir(&clave(1), "sesion", &s),
            Err(ErrorSello::Autenticacion)
        ),
        "…pero no abre"
    );
}

#[test]
fn clave_o_contexto_equivocados_dan_el_mismo_error() {
    let s = sello::sellar(&clave(1), "sesion", 1, b"x").expect("sellar");
    assert!(matches!(
        sello::abrir(&clave(2), "sesion", &s),
        Err(ErrorSello::Autenticacion)
    ));
    assert!(matches!(
        sello::abrir(&clave(1), "config", &s),
        Err(ErrorSello::Autenticacion)
    ));
}

#[test]
fn sellar_dos_veces_lo_mismo_da_sellos_distintos() {
    // Nonce nuevo en cada sello: dos sellos iguales delatarían que los datos lo son.
    let a = sello::sellar(&clave(1), "sesion", 1, b"igual").expect("sellar");
    let b = sello::sellar(&clave(1), "sesion", 1, b"igual").expect("sellar");
    assert_ne!(a, b);
}

#[test]
fn entradas_que_no_son_sellos_dan_errores_claros() {
    assert!(matches!(
        sello::abrir(&clave(1), "x", b""),
        Err(ErrorSello::Truncado { largo: 0 })
    ));
    let mut basura = vec![0u8; LARGO_MINIMO];
    assert!(matches!(
        sello::abrir(&clave(1), "x", &basura),
        Err(ErrorSello::NoEsUnSello)
    ));
    basura[..4].copy_from_slice(b"SCKP");
    basura[4] = 9;
    assert!(matches!(
        sello::abrir(&clave(1), "x", &basura),
        Err(ErrorSello::VersionDesconocida(9))
    ));
}

#[test]
fn ningun_largo_de_entrada_provoca_panico() {
    // Prefijos de un sello válido y bytes arbitrarios de todos los largos cercanos al mínimo.
    let s = sello::sellar(&clave(1), "x", 1, b"contenido").expect("sellar");
    for largo in 0..=s.len() {
        let _ = sello::abrir(&clave(1), "x", &s[..largo]);
    }
    for largo in 0..(LARGO_MINIMO * 2) {
        let bytes: Vec<u8> = (0..largo).map(|i| (i * 31 % 251) as u8).collect();
        let _ = sello::abrir(&clave(1), "x", &bytes);
    }
}

#[test]
fn el_debug_no_muestra_secretos() {
    let c = clave(0xAB);
    assert_eq!(format!("{c:?}"), "Clave(oculta)");
    let s = sello::sellar(&c, "x", 5, b"secreto-muy-secreto").expect("sellar");
    let a = sello::abrir(&c, "x", &s).expect("abrir");
    let d = format!("{a:?}");
    assert!(!d.contains("secreto"), "{d}");
    assert!(d.contains("generacion: 5"), "{d}");
}
