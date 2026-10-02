//! La garantía de solo lectura, comprobada sobre el **código fuente**.
//!
//! Los otros tests prueban lo que el crate hace. Este prueba lo que el crate
//! **no contiene**: recorre `src/` sin comentarios (la documentación tiene que
//! poder nombrar lo prohibido para explicarlo) y falla si aparece
//!
//! - un código de función de escritura Modbus como literal hexadecimal
//!   (5, 6, 15, 16, 21, 22 y 23), o
//! - el nombre de una operación de escritura.
//!
//! También comprueba que el crate no declara dependencias, y que el escaneo de
//! verdad detectaría una violación (un escáner que nunca falla no prueba nada).
//!
//! Los patrones se arman en tiempo de ejecución, y este archivo vive en
//! `tests/`, fuera de lo escaneado.

use std::path::{Path, PathBuf};

/// Códigos de función Modbus que escriben, en decimal.
const CODIGOS_DE_ESCRITURA: [u8; 7] = [5, 6, 15, 16, 21, 22, 23];

/// Fragmentos de nombre que delatarían una operación de escritura. `write_all`
/// no está: es el envío de bytes al socket, que cualquier cliente necesita.
const NOMBRES_DE_ESCRITURA: [&str; 7] =
    ["escrib", "write_register", "write_coil", "write_multiple", "write_single", "preset_", "force_"];

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn archivos_rust(dir: &Path, salida: &mut Vec<PathBuf>) {
    for entrada in std::fs::read_dir(dir).expect("directorio legible") {
        let ruta = entrada.expect("entrada legible").path();
        if ruta.is_dir() {
            archivos_rust(&ruta, salida);
        } else if ruta.extension().is_some_and(|e| e == "rs") {
            salida.push(ruta);
        }
    }
}

/// El código sin comentarios de línea. El crate no usa comentarios de bloque;
/// si alguien los introduce, el test lo pide adaptar en vez de dejar que
/// escondan código.
fn sin_comentarios(ruta: &Path) -> String {
    let texto = std::fs::read_to_string(ruta).expect("fuente legible");
    assert!(!texto.contains("/*"), "{}: comentario de bloque; adaptar el escaneo", ruta.display());
    texto
        .lines()
        .map(|linea| match linea.find("//") {
            Some(i) => &linea[..i],
            None => linea,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Todo lo prohibido que aparece en `codigo`.
fn hallazgos(codigo: &str) -> Vec<String> {
    let codigo = codigo.to_lowercase();
    let mut encontrados = Vec::new();

    for n in CODIGOS_DE_ESCRITURA {
        let patron = format!("0x{n:02x}");
        for (i, _) in codigo.match_indices(&patron) {
            // `0x050` no es `0x05`: el literal tiene que terminar ahí.
            let sigue = codigo[i + patron.len()..].chars().next();
            if !sigue.is_some_and(|c| c.is_ascii_alphanumeric()) {
                encontrados.push(patron.clone());
            }
        }
    }
    for nombre in NOMBRES_DE_ESCRITURA {
        if codigo.contains(nombre) {
            encontrados.push(nombre.to_string());
        }
    }
    encontrados
}

#[test]
fn el_codigo_fuente_no_contiene_ningun_camino_de_escritura() {
    let mut archivos = Vec::new();
    archivos_rust(&raiz().join("src"), &mut archivos);
    assert!(archivos.len() >= 4, "no se encontraron las fuentes: {archivos:?}");

    for ruta in archivos {
        let h = hallazgos(&sin_comentarios(&ruta));
        assert!(h.is_empty(), "{} contiene {h:?}", ruta.display());
    }
}

#[test]
fn el_crate_no_declara_dependencias() {
    let manifiesto = std::fs::read_to_string(raiz().join("Cargo.toml")).expect("Cargo.toml legible");
    let mut en_dependencias = false;
    for linea in manifiesto.lines().map(str::trim) {
        if linea.starts_with('[') {
            en_dependencias = linea == "[dependencies]";
            continue;
        }
        if en_dependencias && !linea.is_empty() && !linea.starts_with('#') {
            panic!("dependencia declarada: {linea}");
        }
    }
}

#[test]
fn el_escaneo_detectaria_una_violacion() {
    let codigo_falso = format!("let f = 0x{:02X};", 6);
    assert_eq!(hallazgos(&codigo_falso), vec!["0x06".to_string()]);

    let nombre_falso = format!("fn {}_bobina() {{}}", "escribir");
    assert_eq!(hallazgos(&nombre_falso), vec!["escrib".to_string()]);

    // Y no confunde literales parecidos.
    assert!(hallazgos("let x = 0x050; let y = 0x03; stream.write_all(&b);").is_empty());
}
