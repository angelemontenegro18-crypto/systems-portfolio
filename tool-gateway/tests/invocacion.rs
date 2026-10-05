//! Lo que pasa cuando el modelo llama mal, o una herramienta se porta mal.

use serde_json::{json, Value};
use tool_gateway::{catalogo, ErrorInvocacion, Herramienta, Limites, Registro};

#[test]
fn una_herramienta_inexistente_es_un_error_no_un_panico() {
    let r = catalogo().expect("catálogo válido");
    assert_eq!(
        r.invocar("borrar_todo", &json!({})),
        Err(ErrorInvocacion::NoExiste("borrar_todo".into()))
    );
}

#[test]
fn argumentos_invalidos_vuelven_con_todos_los_problemas() {
    let r = catalogo().expect("catálogo válido");
    let e = r
        .invocar(
            "estadisticas",
            &json!({"valores": [1, "dos"], "extra": true}),
        )
        .expect_err("inválidos");
    let ErrorInvocacion::ArgumentosInvalidos(problemas) = e else {
        panic!("{e:?}")
    };
    assert_eq!(problemas.len(), 2, "{problemas:?}");
}

#[test]
fn un_error_de_la_herramienta_se_devuelve_como_error() {
    let r = catalogo().expect("catálogo válido");
    let e = r
        .invocar(
            "convertir_temperatura",
            &json!({"valor": -500, "de": "celsius", "a": "kelvin"}),
        )
        .expect_err("bajo el cero absoluto");
    assert!(
        matches!(&e, ErrorInvocacion::Fallo(m) if m.contains("cero absoluto")),
        "{e:?}"
    );
}

fn explota(_: &Value) -> Result<Value, String> {
    panic!("falla de programación dentro de la herramienta")
}

fn gigante(_: &Value) -> Result<Value, String> {
    Ok(json!({"relleno": "x".repeat(10_000)}))
}

fn objeto_vacio() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

#[test]
fn un_panico_de_la_herramienta_no_tumba_el_gateway() {
    let mut r = Registro::nuevo();
    r.registrar(Herramienta {
        nombre: "explota",
        descripcion: "prueba",
        esquema: objeto_vacio(),
        ejemplo: json!({}),
        funcion: explota,
    })
    .expect("registrar");
    assert_eq!(
        r.invocar("explota", &json!({})),
        Err(ErrorInvocacion::Panico)
    );
    // El registro sigue funcionando después.
    assert_eq!(
        r.invocar("explota", &json!({})),
        Err(ErrorInvocacion::Panico)
    );
}

#[test]
fn los_limites_de_tamano_se_aplican_a_la_entrada_y_a_la_salida() {
    let mut r = Registro::con_limites(Limites {
        max_bytes_entrada: 64,
        max_bytes_salida: 1_000,
    });
    r.registrar(Herramienta {
        nombre: "gigante",
        descripcion: "prueba",
        esquema: json!({"type": "object"}),
        ejemplo: json!({}),
        funcion: gigante,
    })
    .expect("registrar");

    let e = r
        .invocar("gigante", &json!({"relleno": "y".repeat(200)}))
        .expect_err("entrada grande");
    assert!(
        matches!(
            e,
            ErrorInvocacion::EntradaDemasiadoGrande { maximo: 64, .. }
        ),
        "{e:?}"
    );

    let e = r.invocar("gigante", &json!({})).expect_err("salida grande");
    assert!(
        matches!(
            e,
            ErrorInvocacion::SalidaDemasiadoGrande { maximo: 1_000, .. }
        ),
        "{e:?}"
    );
}

#[test]
fn responder_arma_el_resultado_para_el_modelo_con_la_marca_de_error() {
    let r = catalogo().expect("catálogo válido");
    let ok = r.responder("llamada_1", "contar_palabras", &json!({"texto": "a b"}));
    assert_eq!(ok["tool_use_id"], "llamada_1");
    assert_eq!(ok["is_error"], false);

    let mal = r.responder("llamada_2", "contar_palabras", &json!({}));
    assert_eq!(mal["is_error"], true);
    assert!(
        mal["content"]
            .as_str()
            .is_some_and(|c| c.contains("falta el campo obligatorio")),
        "{mal}"
    );
}
