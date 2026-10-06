//! Lo que la biblioteca **no contiene**, comprobado sobre su código fuente.
//!
//! Recorre `src/` sin comentarios (la documentación tiene que poder nombrar lo prohibido, y
//! sus ejemplos usan `std`) y falla si aparece
//!
//! - una ruta de `std` o de `alloc`, o un `extern crate`;
//! - `.unwrap()`, `.expect(`, `panic!`, `unreachable!`, `todo!` o `unimplemented!`;
//! - una indexación o un rebanado sin verificar (`x[i]`, `f()[i]`, `&v[a..b]`).
//!
//! También exige que `lib.rs` siga declarando `#![no_std]`, `#![forbid(unsafe_code)]` y los
//! `#![deny(clippy::…)]` que hacen lo mismo en compilación, que el crate no declare
//! dependencias, y que el escaneo de verdad detecte una violación (un escáner que nunca falla
//! no prueba nada). Este archivo vive en `tests/`, fuera de lo escaneado.

use std::path::{Path, PathBuf};

const PROHIBIDOS: [&str; 9] = [
    "std::",
    "alloc::",
    "extern crate",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
];

const ATRIBUTOS_EXIGIDOS: [&str; 10] = [
    "#![no_std]",
    "#![forbid(unsafe_code)]",
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::indexing_slicing",
    "clippy::unreachable",
    "clippy::todo",
    "clippy::unimplemented",
    "#![deny(missing_docs)]",
];

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

/// El código sin comentarios de línea. El crate no usa comentarios de bloque; si alguien los
/// introduce, el test pide adaptar el escaneo en vez de dejar que escondan código.
fn sin_comentarios(texto: &str) -> String {
    assert!(
        !texto.contains("/*"),
        "comentario de bloque; adaptar el escaneo"
    );
    texto
        .lines()
        .map(|linea| match linea.find("//") {
            Some(i) => &linea[..i],
            None => linea,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Todo lo prohibido que aparece en `codigo` (ya sin comentarios).
fn hallazgos(codigo: &str) -> Vec<String> {
    let mut encontrados: Vec<String> = PROHIBIDOS
        .iter()
        .filter(|p| codigo.contains(*p))
        .map(|p| p.to_string())
        .collect();
    // Un corchete pegado a un nombre, a un `)` o a otro `]` es una indexación o un rebanado.
    // Los tipos (`: [i8; 4]`, `&[u8]`), los literales (`= [0; 4]`) y los atributos (`#[`) van
    // precedidos de otra cosa.
    let caracteres: Vec<char> = codigo.chars().collect();
    for (i, ventana) in caracteres.windows(2).enumerate() {
        let (antes, c) = (ventana[0], ventana[1]);
        if c == '[' && (antes.is_alphanumeric() || matches!(antes, '_' | ')' | ']')) {
            let desde = i.saturating_sub(12);
            let contexto: String = caracteres[desde..=i + 1].iter().collect();
            encontrados.push(format!("indexación: …{contexto}"));
        }
    }
    encontrados
}

#[test]
fn la_biblioteca_no_usa_std_ni_unwrap_ni_indexacion_sin_verificar() {
    let mut archivos = Vec::new();
    archivos_rust(&raiz().join("src"), &mut archivos);
    assert!(
        archivos.len() >= 5,
        "no se encontraron las fuentes: {archivos:?}"
    );
    for ruta in archivos {
        let texto = std::fs::read_to_string(&ruta).expect("fuente legible");
        let h = hallazgos(&sin_comentarios(&texto));
        assert!(h.is_empty(), "{} contiene {h:?}", ruta.display());
    }
}

#[test]
fn lib_rs_conserva_no_std_y_las_prohibiciones_del_compilador() {
    let lib = std::fs::read_to_string(raiz().join("src/lib.rs")).expect("lib.rs legible");
    let codigo = sin_comentarios(&lib);
    for atributo in ATRIBUTOS_EXIGIDOS {
        assert!(codigo.contains(atributo), "falta {atributo}");
    }
}

#[test]
fn el_crate_no_declara_dependencias() {
    let manifiesto =
        std::fs::read_to_string(raiz().join("Cargo.toml")).expect("Cargo.toml legible");
    let mut en_dependencias = false;
    for linea in manifiesto.lines().map(str::trim) {
        if linea.starts_with('[') {
            en_dependencias = linea.ends_with("dependencies]");
            continue;
        }
        if en_dependencias && !linea.is_empty() && !linea.starts_with('#') {
            panic!("dependencia declarada: {linea}");
        }
    }
}

#[test]
fn el_escaneo_detectaria_una_violacion() {
    let falso = format!("let x = v.{}();", "unwrap");
    assert_eq!(hallazgos(&falso), vec![".unwrap()".to_string()]);
    let falso = format!("use {}::vec::Vec;", "std");
    assert_eq!(hallazgos(&falso), vec!["std::".to_string()]);

    for indexacion in [
        "let x = v[3];",
        "let y = f()[0];",
        "let s = &a[1..];",
        "m[i][j]",
    ] {
        assert!(!hallazgos(indexacion).is_empty(), "no vio {indexacion}");
    }

    // Y no confunde lo que se parece.
    let limpio = "#[derive(Debug)] let t: [u8; 4] = [0; 4]; let s: &[i8] = &t; \
                  let o = v.unwrap_or(0); use core::fmt; let m: &mut [[i64; 8]] = x;";
    assert!(hallazgos(limpio).is_empty(), "{:?}", hallazgos(limpio));
    assert_eq!(
        sin_comentarios("let a = 1; // v[3].unwrap()"),
        "let a = 1; "
    );
}
