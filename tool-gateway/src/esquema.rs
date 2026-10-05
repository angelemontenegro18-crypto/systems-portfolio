//! Validación de argumentos contra un subconjunto de JSON Schema.
//!
//! Un modelo de lenguaje produce argumentos que *parecen* correctos y a veces
//! no lo son: un número como texto, un campo que falta, un arreglo de más. Nada
//! de eso llega a una herramienta: se valida antes y el error dice exactamente
//! dónde (con una ruta tipo JSON Pointer, `/valores/3`).
//!
//! El subconjunto es deliberado y **cerrado**: un esquema que usa una palabra
//! clave que este validador no aplica se rechaza al registrar la herramienta.
//! Un esquema no puede prometer una restricción que nadie controla.

use serde_json::{Map, Value};

/// Palabras clave que este validador sabe aplicar.
const SOPORTADAS: &[&str] = &[
    "type",
    "description",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "enum",
];

/// Comprueba que el esquema solo use palabras clave soportadas, en todos sus niveles.
pub fn revisar_esquema(esquema: &Value) -> Result<(), String> {
    revisar_en(esquema, "")
}

fn revisar_en(esquema: &Value, ruta: &str) -> Result<(), String> {
    let Some(obj) = esquema.as_object() else {
        return Err(format!(
            "{}: el esquema tiene que ser un objeto",
            mostrar(ruta)
        ));
    };
    for clave in obj.keys() {
        if !SOPORTADAS.contains(&clave.as_str()) {
            return Err(format!(
                "{}: palabra clave no soportada `{clave}`",
                mostrar(ruta)
            ));
        }
    }
    if let Some(Value::Bool(true)) = obj.get("additionalProperties") {
        return Err(format!(
            "{}: `additionalProperties` solo admite `false`",
            mostrar(ruta)
        ));
    }
    if let Some(props) = obj.get("properties") {
        let Some(props) = props.as_object() else {
            return Err(format!(
                "{}: `properties` tiene que ser un objeto",
                mostrar(ruta)
            ));
        };
        for (nombre, sub) in props {
            revisar_en(sub, &format!("{ruta}/{nombre}"))?;
        }
    }
    if let Some(items) = obj.get("items") {
        revisar_en(items, &format!("{ruta}/items"))?;
    }
    Ok(())
}

/// Valida `valor` contra `esquema`. Devuelve todos los problemas, no solo el primero:
/// al modelo le sirve ver la lista completa para corregir en un solo intento.
pub fn validar(esquema: &Value, valor: &Value) -> Result<(), Vec<String>> {
    let mut errores = Vec::new();
    validar_en(esquema, valor, "", &mut errores);
    if errores.is_empty() {
        Ok(())
    } else {
        Err(errores)
    }
}

fn mostrar(ruta: &str) -> &str {
    if ruta.is_empty() {
        "/"
    } else {
        ruta
    }
}

fn nombre_de_tipo(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn cumple_tipo(tipo: &str, v: &Value) -> bool {
    match tipo {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "null" => v.is_null(),
        _ => false,
    }
}

fn validar_en(esquema: &Value, valor: &Value, ruta: &str, errores: &mut Vec<String>) {
    let Some(e) = esquema.as_object() else { return };
    let donde = mostrar(ruta);

    if let Some(tipo) = e.get("type").and_then(Value::as_str) {
        if !cumple_tipo(tipo, valor) {
            errores.push(format!(
                "{donde}: se esperaba {tipo} y llegó {}",
                nombre_de_tipo(valor)
            ));
            return;
        }
    }

    if let Some(opciones) = e.get("enum").and_then(Value::as_array) {
        if !opciones.contains(valor) {
            errores.push(format!(
                "{donde}: {valor} no es una de las opciones {}",
                Value::Array(opciones.clone())
            ));
        }
    }

    match valor {
        Value::Object(obj) => validar_objeto(e, obj, ruta, errores),
        Value::Array(elementos) => {
            let n = elementos.len() as u64;
            if let Some(min) = e.get("minItems").and_then(Value::as_u64) {
                if n < min {
                    errores.push(format!("{donde}: {n} elemento(s), el mínimo es {min}"));
                }
            }
            if let Some(max) = e.get("maxItems").and_then(Value::as_u64) {
                if n > max {
                    errores.push(format!("{donde}: {n} elemento(s), el máximo es {max}"));
                }
            }
            if let Some(items) = e.get("items") {
                for (i, elemento) in elementos.iter().enumerate() {
                    validar_en(items, elemento, &format!("{ruta}/{i}"), errores);
                }
            }
        }
        Value::String(s) => {
            let n = s.chars().count() as u64;
            if let Some(min) = e.get("minLength").and_then(Value::as_u64) {
                if n < min {
                    errores.push(format!("{donde}: {n} carácter(es), el mínimo es {min}"));
                }
            }
            if let Some(max) = e.get("maxLength").and_then(Value::as_u64) {
                if n > max {
                    errores.push(format!("{donde}: {n} carácter(es), el máximo es {max}"));
                }
            }
        }
        Value::Number(num) => {
            let Some(x) = num.as_f64() else { return };
            if let Some(min) = e.get("minimum").and_then(Value::as_f64) {
                if x < min {
                    errores.push(format!("{donde}: {x} es menor que el mínimo {min}"));
                }
            }
            if let Some(max) = e.get("maximum").and_then(Value::as_f64) {
                if x > max {
                    errores.push(format!("{donde}: {x} es mayor que el máximo {max}"));
                }
            }
        }
        _ => {}
    }
}

fn validar_objeto(
    e: &Map<String, Value>,
    obj: &Map<String, Value>,
    ruta: &str,
    errores: &mut Vec<String>,
) {
    let props = e.get("properties").and_then(Value::as_object);

    if let Some(requeridos) = e.get("required").and_then(Value::as_array) {
        for r in requeridos.iter().filter_map(Value::as_str) {
            if !obj.contains_key(r) {
                errores.push(format!(
                    "{}: falta el campo obligatorio `{r}`",
                    mostrar(ruta)
                ));
            }
        }
    }

    let cerrado = matches!(e.get("additionalProperties"), Some(Value::Bool(false)));
    for (nombre, valor) in obj {
        match props.and_then(|p| p.get(nombre)) {
            Some(sub) => validar_en(sub, valor, &format!("{ruta}/{nombre}"), errores),
            None if cerrado => {
                errores.push(format!("{}: campo no permitido `{nombre}`", mostrar(ruta)))
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn esquema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "valores": {"type": "array", "items": {"type": "number"}, "minItems": 1, "maxItems": 3},
                "modo": {"type": "string", "enum": ["rapido", "exacto"]},
                "n": {"type": "integer", "minimum": 1, "maximum": 10}
            },
            "required": ["valores"],
            "additionalProperties": false
        })
    }

    #[test]
    fn un_argumento_valido_pasa() {
        assert_eq!(
            validar(
                &esquema(),
                &json!({"valores": [1, 2.5], "modo": "rapido", "n": 3})
            ),
            Ok(())
        );
    }

    #[test]
    fn junta_todos_los_errores_con_su_ruta() {
        let e = validar(
            &esquema(),
            &json!({"valores": [1, "dos", 3, 4], "modo": "lento", "n": 0, "x": 1}),
        )
        .expect_err("varios errores");
        let texto = e.join(" | ");
        for esperado in [
            "/valores: 4 elemento(s), el máximo es 3",
            "/valores/1: se esperaba number y llegó string",
            "/modo: \"lento\" no es una de las opciones",
            "/n: 0 es menor que el mínimo 1",
            "/: campo no permitido `x`",
        ] {
            assert!(texto.contains(esperado), "falta «{esperado}» en: {texto}");
        }
    }

    #[test]
    fn falta_un_obligatorio() {
        let e = validar(&esquema(), &json!({})).expect_err("falta valores");
        assert_eq!(
            e,
            vec!["/: falta el campo obligatorio `valores`".to_string()]
        );
    }

    #[test]
    fn un_entero_no_acepta_decimales_pero_un_numero_si() {
        let s = json!({"type": "object", "properties": {"i": {"type": "integer"}, "x": {"type": "number"}}});
        assert!(validar(&s, &json!({"i": 2, "x": 2})).is_ok());
        assert!(validar(&s, &json!({"i": 2.5})).is_err());
    }

    #[test]
    fn un_esquema_con_palabras_clave_desconocidas_se_rechaza() {
        let s = json!({"type": "object", "properties": {"c": {"type": "string", "pattern": "^a"}}});
        assert_eq!(
            revisar_esquema(&s),
            Err("/c: palabra clave no soportada `pattern`".to_string())
        );
        assert!(revisar_esquema(&esquema()).is_ok());
    }
}
