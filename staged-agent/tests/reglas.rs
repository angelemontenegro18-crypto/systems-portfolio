//! Las reglas de normalización.

use staged_agent::{reglas_por_defecto, EspaciosFinales, FinesDeLinea, Regla, SaltoFinal};

#[test]
fn fines_de_linea_convierte_crlf_y_deja_los_cr_sueltos() {
    let r = FinesDeLinea;
    assert_eq!(r.aplicar("a\r\nb\r\n"), "a\nb\n");
    assert_eq!(r.aplicar("doble\r\r\nconversion"), "doble\nconversion");
    assert_eq!(r.aplicar("a\rb\n"), "a\rb\n");
    assert_eq!(r.aplicar("termina en cr\r"), "termina en cr\r");
}

#[test]
fn espacios_finales_quita_espacios_y_tabs_al_final_de_cada_linea() {
    let r = EspaciosFinales;
    assert_eq!(r.aplicar("a  \nb\t\n  c \n"), "a\nb\n  c\n");
    assert_eq!(r.aplicar("sin salto   "), "sin salto");
    // Los CR que terminan la línea se conservan: son de otra regla.
    assert_eq!(r.aplicar("a \r\nb"), "a\r\nb");
    assert_eq!(r.aplicar("a \r \r\n"), "a\r\n");
    // Un CR seguido de blanco no termina nada: se va con el blanco.
    assert_eq!(r.aplicar("a\r\t \nb"), "a\nb");
    // La sangría no es un espacio final.
    assert_eq!(r.aplicar("\tsangria"), "\tsangria");
}

#[test]
fn salto_final_deja_exactamente_uno() {
    let r = SaltoFinal;
    assert_eq!(r.aplicar("a"), "a\n");
    assert_eq!(r.aplicar("a\n"), "a\n");
    assert_eq!(r.aplicar("a\n\n\n"), "a\n");
    assert_eq!(r.aplicar("a\r\n\r\n"), "a\r\n");
    assert_eq!(r.aplicar(""), "");
    assert_eq!(r.aplicar("\n\n"), "");
}

/// Un generador mínimo para armar textos con los caracteres conflictivos.
fn textos_al_azar() -> Vec<String> {
    let alfabeto = ['a', 'ñ', ' ', '\t', '\r', '\n'];
    let mut estado: u64 = 0x2545_F491_4F6C_DD1D;
    let mut siguiente = move || {
        estado ^= estado << 13;
        estado ^= estado >> 7;
        estado ^= estado << 17;
        estado
    };
    (0..2000)
        .map(|_| {
            let largo = (siguiente() % 24) as usize;
            (0..largo)
                .map(|_| alfabeto[(siguiente() % alfabeto.len() as u64) as usize])
                .collect()
        })
        .collect()
}

#[test]
fn las_reglas_y_su_composicion_son_idempotentes() {
    let reglas = reglas_por_defecto();
    let componer = |texto: &str| reglas.iter().fold(texto.to_string(), |t, r| r.aplicar(&t));
    for texto in textos_al_azar() {
        for regla in &reglas {
            let una = regla.aplicar(&texto);
            assert_eq!(
                regla.aplicar(&una),
                una,
                "{} no es idempotente con {texto:?}",
                regla.nombre()
            );
        }
        let una = componer(&texto);
        assert_eq!(
            componer(&una),
            una,
            "la composición no es idempotente con {texto:?}"
        );
    }
}

#[test]
fn un_texto_normalizado_no_cambia() {
    let texto = "primera línea\n\tsegunda, con sangría\n";
    for regla in reglas_por_defecto() {
        assert_eq!(regla.aplicar(texto), texto, "{}", regla.nombre());
    }
}
