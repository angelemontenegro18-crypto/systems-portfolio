//! La política de exposición, verificada sobre el catálogo real.

use serde_json::{json, Value};
use tool_gateway::politica::EXCLUIDAS;
use tool_gateway::{catalogo, ErrorRegistro, Herramienta, Registro};

fn nada(_: &Value) -> Result<Value, String> {
    Ok(json!({}))
}

fn herramienta(nombre: &'static str) -> Herramienta {
    Herramienta {
        nombre,
        descripcion: "prueba",
        esquema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        ejemplo: json!({}),
        funcion: nada,
    }
}

#[test]
fn ninguna_herramienta_vetada_esta_en_el_catalogo() {
    let r = catalogo().expect("catálogo válido");
    for v in EXCLUIDAS {
        assert!(!r.nombres().contains(&v.nombre), "`{}` está vetada y aparece registrada", v.nombre);
    }
}

#[test]
fn el_registro_se_niega_a_aceptar_cada_herramienta_vetada() {
    let mut r = Registro::nuevo();
    for v in EXCLUIDAS {
        match r.registrar(herramienta(v.nombre)) {
            Err(ErrorRegistro::Vetada { nombre, motivo }) => {
                assert_eq!(nombre, v.nombre);
                assert!(!motivo.is_empty(), "todo veto explica su motivo");
            }
            otro => panic!("`{}` debería estar vetada: {otro:?}", v.nombre),
        }
    }
    assert!(r.is_empty());
}

#[test]
fn cada_herramienta_tiene_un_ejemplo_que_cumple_su_esquema() {
    // `registrar` ya lo exige; esto lo confirma para el catálogo publicado.
    for h in tool_gateway::herramientas() {
        assert!(
            tool_gateway::esquema::validar(&h.esquema, &h.ejemplo).is_ok(),
            "el ejemplo de `{}` no cumple su esquema",
            h.nombre
        );
    }
}

#[test]
fn cada_herramienta_es_determinista() {
    // Llamada dos veces con la misma entrada, tiene que dar lo mismo. No prueba
    // la pureza (nada puede probarla desde afuera), pero atrapa el caso típico:
    // una herramienta que lee la hora, el azar o un contador global.
    let r = catalogo().expect("catálogo válido");
    for h in r.herramientas() {
        let a = r.invocar(h.nombre, &h.ejemplo);
        let b = r.invocar(h.nombre, &h.ejemplo);
        assert!(a.is_ok(), "`{}` falla con su propio ejemplo: {a:?}", h.nombre);
        assert_eq!(a, b, "`{}` no es determinista", h.nombre);
    }
}

#[test]
fn el_manifiesto_tiene_la_forma_de_function_calling() {
    let m = catalogo().expect("catálogo válido").manifiesto();
    let lista = m.as_array().expect("arreglo");
    assert_eq!(lista.len(), 6);
    for t in lista {
        assert!(t["name"].is_string() && t["description"].is_string(), "{t}");
        assert_eq!(t["input_schema"]["type"], "object", "{t}");
    }
    // Ordenado por nombre: el manifiesto es estable entre ejecuciones.
    let nombres: Vec<&str> = lista.iter().filter_map(|t| t["name"].as_str()).collect();
    let mut ordenados = nombres.clone();
    ordenados.sort_unstable();
    assert_eq!(nombres, ordenados);
}

#[test]
fn nombres_duplicados_invalidos_y_esquemas_que_prometen_de_mas_se_rechazan() {
    let mut r = Registro::nuevo();
    r.registrar(herramienta("una")).expect("primera");
    assert_eq!(r.registrar(herramienta("una")), Err(ErrorRegistro::Duplicada("una".into())));
    assert!(matches!(r.registrar(herramienta("Con-Mayus")), Err(ErrorRegistro::NombreInvalido(_))));

    let mut con_pattern = herramienta("patron");
    con_pattern.esquema = json!({"type": "object", "properties": {"c": {"type": "string", "pattern": "^a"}}});
    con_pattern.ejemplo = json!({"c": "a"});
    assert!(matches!(r.registrar(con_pattern), Err(ErrorRegistro::EsquemaInvalido { .. })));

    let mut mal_ejemplo = herramienta("mal_ejemplo");
    mal_ejemplo.ejemplo = json!({"extra": 1});
    assert!(matches!(r.registrar(mal_ejemplo), Err(ErrorRegistro::EjemploInvalido { .. })));
}
