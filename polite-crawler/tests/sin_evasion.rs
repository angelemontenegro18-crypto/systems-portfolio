//! Lo que este crawler **no contiene**, comprobado sobre el código fuente.
//!
//! Recorre `src/` sin comentarios (la documentación tiene que poder nombrar lo
//! que no se hace, para decirlo) y falla si aparece cualquier mecanismo de
//! ocultamiento: proxies, rotación o suplantación de identidad, navegadores
//! automatizados, tiempos aleatorios, o alguna forma de desactivar `robots.txt`.
//! También verifica que las dependencias sean exactamente las declaradas, y que
//! el escaneo de verdad detectaría una violación.
//!
//! Los patrones se arman a partir de fragmentos, y este archivo vive en
//! `tests/`, fuera de lo escaneado.

use std::path::{Path, PathBuf};

fn prohibidos() -> Vec<String> {
    [
        ("pro", "xy"),
        ("rota", "te"),
        ("rot", "ar"),
        ("spo", "of"),
        ("suplan", "t"),
        ("head", "less"),
        ("finger", "print"),
        ("web", "driver"),
        ("capt", "cha"),
        ("jit", "ter"),
        ("ran", "d::"),
        ("alea", "tori"),
        ("ignor", "ar_robots"),
        ("ignore", "_robots"),
        ("respe", "tar_robots"),
        ("respect", "_robots"),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

fn archivos(dir: &Path, salida: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("legible") {
        let p = e.expect("entrada").path();
        if p.is_dir() {
            archivos(&p, salida);
        } else if p.extension().is_some_and(|x| x == "rs") {
            salida.push(p);
        }
    }
}

/// Código sin comentarios de línea, en minúsculas. Los comentarios de bloque
/// no se usan; si aparecen, el test pide adaptar el escaneo.
///
/// `/*` sola no alcanza para detectarlos: en este crate aparece legítimamente
/// dentro de strings, porque `/*.php` es un patrón de `robots.txt`. Un
/// comentario de bloque real empieza con `/*` seguido de espacio, `*` o `!`.
fn codigo(p: &Path) -> String {
    let t = std::fs::read_to_string(p).expect("fuente");
    let abre_bloque = ["/* ", "/*\n", "/**", "/*!"].iter().any(|m| t.contains(m));
    assert!(
        !abre_bloque,
        "{}: comentario de bloque; adaptar el escaneo",
        p.display()
    );
    t.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

fn hallazgos(texto: &str) -> Vec<String> {
    prohibidos()
        .into_iter()
        .filter(|p| texto.contains(p.as_str()))
        .collect()
}

#[test]
fn el_codigo_no_contiene_mecanismos_de_evasion() {
    let mut v = Vec::new();
    archivos(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut v,
    );
    assert!(v.len() >= 5, "no se encontraron las fuentes");
    for p in v {
        let h = hallazgos(&codigo(&p));
        assert!(h.is_empty(), "{} contiene {h:?}", p.display());
    }
}

#[test]
fn las_dependencias_son_exactamente_las_declaradas() {
    let manifiesto =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("Cargo.toml");
    let mut en_deps = false;
    let mut deps = Vec::new();
    for linea in manifiesto.lines().map(str::trim) {
        if linea.starts_with('[') {
            en_deps = linea == "[dependencies]";
        } else if en_deps && !linea.is_empty() && !linea.starts_with('#') {
            deps.push(linea.split('=').next().unwrap_or("").trim().to_string());
        }
    }
    assert_eq!(
        deps,
        vec!["ureq", "url"],
        "una dependencia nueva tiene que pasar por revisión"
    );
}

#[test]
fn el_escaneo_detectaria_una_violacion() {
    // Alguien intenta agregar un interruptor para ignorar robots.txt.
    let codigo_falso = format!(
        "pub struct Config {{ pub {}: bool }}",
        "respect".to_owned() + "_robots"
    );
    assert_eq!(
        hallazgos(&codigo_falso),
        vec![format!("{}{}", "respect", "_robots")]
    );
    let otro = format!("fn {}(&mut self) {{}}", "rotar_".to_owned() + "agente");
    assert!(!hallazgos(&otro).is_empty());
    assert!(hallazgos("fn obtener(&mut self, url: &str) {}").is_empty());
}
