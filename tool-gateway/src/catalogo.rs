//! Un catálogo de ejemplo: seis funciones puras, útiles para un asistente y
//! sin ningún efecto. Sirven para mostrar la forma de una herramienta bien
//! declarada: nombre, descripción, esquema cerrado y un ejemplo válido.

use serde_json::{json, Value};

use crate::registro::{ErrorRegistro, Herramienta, Registro};

/// Un registro con las seis herramientas de ejemplo.
pub fn catalogo() -> Result<Registro, ErrorRegistro> {
    let mut r = Registro::nuevo();
    for h in herramientas() {
        r.registrar(h)?;
    }
    Ok(r)
}

/// Las herramientas de ejemplo, sin registrar.
pub fn herramientas() -> Vec<Herramienta> {
    vec![
        Herramienta {
            nombre: "crc32",
            descripcion: "Calcula el CRC-32 (IEEE) de un texto, para comparar integridad.",
            esquema: json!({
                "type": "object",
                "properties": {"texto": {"type": "string", "maxLength": 100000}},
                "required": ["texto"],
                "additionalProperties": false
            }),
            ejemplo: json!({"texto": "hola"}),
            funcion: crc32,
        },
        Herramienta {
            nombre: "estadisticas",
            descripcion: "Media, mediana, desviación estándar, mínimo y máximo de una lista de números.",
            esquema: json!({
                "type": "object",
                "properties": {"valores": {"type": "array", "items": {"type": "number"}, "minItems": 1, "maxItems": 10000}},
                "required": ["valores"],
                "additionalProperties": false
            }),
            ejemplo: json!({"valores": [2, 4, 4, 4, 5, 5, 7, 9]}),
            funcion: estadisticas,
        },
        Herramienta {
            nombre: "convertir_temperatura",
            descripcion: "Convierte una temperatura entre celsius, fahrenheit y kelvin.",
            esquema: json!({
                "type": "object",
                "properties": {
                    "valor": {"type": "number"},
                    "de": {"type": "string", "enum": ["celsius", "fahrenheit", "kelvin"]},
                    "a": {"type": "string", "enum": ["celsius", "fahrenheit", "kelvin"]}
                },
                "required": ["valor", "de", "a"],
                "additionalProperties": false
            }),
            ejemplo: json!({"valor": 100, "de": "celsius", "a": "fahrenheit"}),
            funcion: convertir_temperatura,
        },
        Herramienta {
            nombre: "validar_isbn",
            descripcion: "Verifica el dígito de control de un ISBN-10 o ISBN-13 (acepta guiones y espacios).",
            esquema: json!({
                "type": "object",
                "properties": {"isbn": {"type": "string", "minLength": 10, "maxLength": 32}},
                "required": ["isbn"],
                "additionalProperties": false
            }),
            ejemplo: json!({"isbn": "978-84-376-0494-7"}),
            funcion: validar_isbn,
        },
        Herramienta {
            nombre: "contar_palabras",
            descripcion: "Cuenta palabras, caracteres y líneas de un texto.",
            esquema: json!({
                "type": "object",
                "properties": {"texto": {"type": "string", "maxLength": 100000}},
                "required": ["texto"],
                "additionalProperties": false
            }),
            ejemplo: json!({"texto": "uno dos\ntres"}),
            funcion: contar_palabras,
        },
        Herramienta {
            nombre: "distancia",
            descripcion: "Distancia euclidiana entre dos puntos del plano.",
            esquema: json!({
                "type": "object",
                "properties": {
                    "a": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"], "additionalProperties": false},
                    "b": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"], "additionalProperties": false}
                },
                "required": ["a", "b"],
                "additionalProperties": false
            }),
            ejemplo: json!({"a": {"x": 0, "y": 0}, "b": {"x": 3, "y": 4}}),
            funcion: distancia,
        },
    ]
}

// ─── Implementaciones ────────────────────────────────────────────────────
// Reciben argumentos ya validados contra su esquema, pero no confían en eso
// para no entrar en pánico: un campo ausente es un error, no un `unwrap`.

fn texto<'a>(args: &'a Value, campo: &str) -> Result<&'a str, String> {
    args.get(campo).and_then(Value::as_str).ok_or_else(|| format!("falta `{campo}`"))
}

fn numero(args: &Value, campo: &str) -> Result<f64, String> {
    args.get(campo).and_then(Value::as_f64).ok_or_else(|| format!("falta `{campo}`"))
}

fn crc32(args: &Value) -> Result<Value, String> {
    let mut crc: u32 = !0;
    for &b in texto(args, "texto")?.as_bytes() {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    let crc = !crc;
    Ok(json!({"crc32": crc, "hex": format!("{crc:08X}")}))
}

fn estadisticas(args: &Value) -> Result<Value, String> {
    let mut v: Vec<f64> = args
        .get("valores")
        .and_then(Value::as_array)
        .ok_or("falta `valores`")?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    if v.is_empty() {
        return Err("hace falta al menos un valor".into());
    }
    let n = v.len() as f64;
    let media = v.iter().sum::<f64>() / n;
    let varianza = v.iter().map(|x| (x - media).powi(2)).sum::<f64>() / n;
    v.sort_by(f64::total_cmp);
    let medio = v.len() / 2;
    let mediana = if v.len().is_multiple_of(2) { (v[medio - 1] + v[medio]) / 2.0 } else { v[medio] };
    Ok(json!({
        "n": v.len(),
        "media": media,
        "mediana": mediana,
        "desviacion_estandar": varianza.sqrt(),
        "minimo": v[0],
        "maximo": v[v.len() - 1]
    }))
}

fn convertir_temperatura(args: &Value) -> Result<Value, String> {
    let valor = numero(args, "valor")?;
    let kelvin = match texto(args, "de")? {
        "celsius" => valor + 273.15,
        "fahrenheit" => (valor - 32.0) * 5.0 / 9.0 + 273.15,
        "kelvin" => valor,
        otra => return Err(format!("unidad desconocida `{otra}`")),
    };
    if kelvin < 0.0 {
        return Err("la temperatura está por debajo del cero absoluto".into());
    }
    let resultado = match texto(args, "a")? {
        "celsius" => kelvin - 273.15,
        "fahrenheit" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
        "kelvin" => kelvin,
        otra => return Err(format!("unidad desconocida `{otra}`")),
    };
    // Redondeo a 6 decimales: evita mostrar 211.99999999999997 por un 212.
    Ok(json!({"valor": (resultado * 1e6).round() / 1e6}))
}

fn validar_isbn(args: &Value) -> Result<Value, String> {
    let limpio: Vec<char> = texto(args, "isbn")?.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    let (valido, formato) = match limpio.len() {
        10 => {
            let mut suma = 0u32;
            let mut ok = true;
            for (i, c) in limpio.iter().enumerate() {
                let d = match (i, c) {
                    (9, 'X' | 'x') => 10,
                    (_, c) => match c.to_digit(10) {
                        Some(d) => d,
                        None => {
                            ok = false;
                            break;
                        }
                    },
                };
                suma += d * (10 - i as u32);
            }
            (ok && suma.is_multiple_of(11), "isbn-10")
        }
        13 => {
            let digitos: Option<Vec<u32>> = limpio.iter().map(|c| c.to_digit(10)).collect();
            let valido = digitos.is_some_and(|d| {
                d.iter().enumerate().map(|(i, x)| if i.is_multiple_of(2) { *x } else { x * 3 }).sum::<u32>().is_multiple_of(10)
            });
            (valido, "isbn-13")
        }
        _ => (false, "desconocido"),
    };
    Ok(json!({"valido": valido, "formato": formato}))
}

fn contar_palabras(args: &Value) -> Result<Value, String> {
    let t = texto(args, "texto")?;
    Ok(json!({
        "palabras": t.split_whitespace().count(),
        "caracteres": t.chars().count(),
        "lineas": if t.is_empty() { 0 } else { t.lines().count() }
    }))
}

fn distancia(args: &Value) -> Result<Value, String> {
    let punto = |campo: &str| -> Result<(f64, f64), String> {
        let p = args.get(campo).ok_or_else(|| format!("falta `{campo}`"))?;
        Ok((numero(p, "x")?, numero(p, "y")?))
    };
    let ((ax, ay), (bx, by)) = (punto("a")?, punto("b")?);
    Ok(json!({"distancia": (bx - ax).hypot(by - ay)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_herramienta_devuelve_lo_esperado_para_su_ejemplo() {
        let r = catalogo().expect("catálogo válido");
        let esperados = [
            // Valor de referencia calculado con zlib, no con este código.
            ("crc32", json!({"crc32": 1_872_820_616u32, "hex": "6FA0F988"})),
            ("contar_palabras", json!({"palabras": 3, "caracteres": 12, "lineas": 2})),
            ("distancia", json!({"distancia": 5.0})),
            ("validar_isbn", json!({"valido": true, "formato": "isbn-13"})),
            ("convertir_temperatura", json!({"valor": 212.0})),
        ];
        for (nombre, esperado) in esperados {
            let h = r.herramientas().find(|h| h.nombre == nombre).expect("existe");
            assert_eq!(r.invocar(nombre, &h.ejemplo), Ok(esperado), "{nombre}");
        }
    }

    #[test]
    fn las_estadisticas_son_correctas() {
        let v = estadisticas(&json!({"valores": [2, 4, 4, 4, 5, 5, 7, 9]})).expect("ok");
        assert_eq!(v["media"], json!(5.0));
        assert_eq!(v["mediana"], json!(4.5));
        assert_eq!(v["desviacion_estandar"], json!(2.0));
    }

    #[test]
    fn un_isbn_con_digito_de_control_equivocado_no_es_valido() {
        assert_eq!(validar_isbn(&json!({"isbn": "0-306-40615-2"})), Ok(json!({"valido": true, "formato": "isbn-10"})));
        assert_eq!(validar_isbn(&json!({"isbn": "0-306-40615-3"})), Ok(json!({"valido": false, "formato": "isbn-10"})));
    }

    #[test]
    fn por_debajo_del_cero_absoluto_es_un_error_de_la_herramienta() {
        assert!(convertir_temperatura(&json!({"valor": -300, "de": "celsius", "a": "kelvin"})).is_err());
    }
}
