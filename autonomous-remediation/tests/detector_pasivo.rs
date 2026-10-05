//! El detector es pasivo, comprobado sobre el **código fuente**.
//!
//! Recorre `src/detector.rs` sin comentarios y falla si aparece cualquier forma
//! de E/S, de lanzar procesos o hilos, de guardar estado oculto, o de llegar al
//! resto del crate (el actuador incluido). Los únicos `use` admitidos están en
//! una lista. Exige además que el análisis reciba `&[u64]`.
//!
//! Los patrones se arman en tiempo de ejecución, y este archivo vive en
//! `tests/`, fuera de lo escaneado.

use std::path::Path;

const PROHIBIDO: [&str; 22] = [
    "std::net",
    "std::fs",
    "std::io",
    "std::process",
    "std::env",
    "std::thread",
    "std::os",
    "TcpStream",
    "UdpSocket",
    "File",
    "Command",
    "print!",
    "println!",
    "eprint",
    "dbg!",
    "static mut",
    "Cell",
    "Mutex",
    "Atomic",
    "crate::",
    "Actuador",
    "unsafe",
];

const USE_ADMITIDOS: [&str; 1] = ["use std::f64::consts::LN_2;"];

fn fuente() -> String {
    let ruta = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("detector.rs");
    std::fs::read_to_string(ruta).expect("leer src/detector.rs")
}

/// El código sin comentarios de línea. El crate no usa comentarios de bloque;
/// si alguien los introduce, el test pide adaptar el escaneo.
fn sin_comentarios(texto: &str) -> String {
    assert!(
        !texto.contains("/*"),
        "comentario de bloque: adaptar el escaneo"
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

fn hallazgos(codigo: &str) -> Vec<String> {
    let mut encontrados: Vec<String> = PROHIBIDO
        .iter()
        .filter(|p| codigo.contains(*p))
        .map(|p| p.to_string())
        .collect();
    for linea in codigo.lines().map(str::trim) {
        if linea.starts_with("use ") && !USE_ADMITIDOS.contains(&linea) {
            encontrados.push(linea.to_string());
        }
    }
    encontrados
}

#[test]
fn el_detector_no_tiene_ninguna_forma_de_hacer_e_s() {
    let codigo = sin_comentarios(&fuente());
    let h = hallazgos(&codigo);
    assert!(h.is_empty(), "src/detector.rs contiene {h:?}");
    assert!(
        codigo.contains("pub fn analizar(&self, intervalos: &[u64]) -> Analisis"),
        "el análisis tiene que recibir los tiempos ya observados"
    );
}

#[test]
fn el_escaneo_detectaria_una_violacion() {
    let red = format!("let s = std::{}::TcpStream::connect(destino);", "net");
    assert!(hallazgos(&red).contains(&"std::net".to_string()));
    let otro_use = format!("use crate::{};", "Remediador");
    let h = hallazgos(&otro_use);
    assert!(
        h.contains(&"crate::".to_string()) && h.contains(&otro_use),
        "{h:?}"
    );
    assert!(hallazgos("use std::f64::consts::LN_2;\nlet x = 1;").is_empty());
}
