//! La simulación es de solo lectura, comprobado sobre el **código fuente**.
//!
//! Los otros tests prueban lo que la simulación hace. Este prueba lo que su
//! código **no contiene**: recorre, sin comentarios, los módulos que usa
//! (`simulacion`, `lectura`, `plan`, `regla`, `rutas`) y falla si aparece una
//! API de escritura de archivos, un proceso lanzado por fuera de `lectura`, o
//! una llamada a los módulos que escriben. En `lectura` —el único que habla con
//! `git`— exige además que cada cadena literal esté en una lista de argumentos
//! de solo lectura: un `"commit"` o un `"worktree"` nuevo lo hace fallar.
//!
//! Los patrones se arman en tiempo de ejecución, y este archivo vive en
//! `tests/`, fuera de lo escaneado.

use std::path::{Path, PathBuf};

const MODULOS_DE_SIMULACION: [&str; 5] = [
    "simulacion.rs",
    "lectura.rs",
    "plan.rs",
    "regla.rs",
    "rutas.rs",
];

/// Formas de escribir, de lanzar procesos o de llegar a los módulos que escriben.
const PROHIBIDO: [&str; 21] = [
    "fs::write",
    "File::create",
    "OpenOptions",
    "remove_file",
    "remove_dir",
    "create_dir",
    "fs::rename",
    "fs::copy",
    "set_permissions",
    "hard_link",
    "write_all",
    ".write(",
    "set_len",
    "sync_all",
    "Command::new",
    ".spawn(",
    "process::",
    "preparacion",
    "aplicacion",
    "Limpieza",
    "unsafe",
];

/// Lo único que `lectura` puede pasarle a `git`.
const LITERALES_DE_LECTURA: [&str; 18] = [
    "rev-parse",
    "merge-base",
    "--is-ancestor",
    "--verify",
    "--quiet",
    "--symbolic-full-name",
    "HEAD",
    "refs/heads/",
    "refs/heads/{rama}^{{commit}}",
    "{commit}^@",
    "ls-tree",
    "-r",
    "-z",
    "--full-tree",
    "cat-file",
    "blob",
    "diff-tree",
    "--no-renames",
];

fn fuente(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(nombre)
}

/// El código sin comentarios de línea. El crate no usa comentarios de bloque;
/// si alguien los introduce, el test pide adaptar el escaneo en vez de dejar que
/// escondan código.
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

/// Las cadenas literales entre comillas dobles de `codigo`.
fn literales(codigo: &str) -> Vec<String> {
    let mut encontrados = Vec::new();
    let mut caracteres = codigo.chars().peekable();
    while let Some(c) = caracteres.next() {
        match c {
            // Un carácter literal: se salta entero, para que '"' no abra una cadena.
            '\'' => {
                let mut previo = c;
                for siguiente in caracteres.by_ref() {
                    if siguiente == '\'' && previo != '\\' {
                        break;
                    }
                    previo = siguiente;
                }
            }
            '"' => {
                let mut literal = String::new();
                while let Some(siguiente) = caracteres.next() {
                    match siguiente {
                        '\\' => {
                            literal.push(siguiente);
                            if let Some(escapado) = caracteres.next() {
                                literal.push(escapado);
                            }
                        }
                        '"' => break,
                        otro => literal.push(otro),
                    }
                }
                encontrados.push(literal);
            }
            _ => {}
        }
    }
    encontrados
}

fn hallazgos(codigo: &str, es_lectura: bool) -> Vec<String> {
    let mut encontrados: Vec<String> = PROHIBIDO
        .iter()
        .filter(|p| codigo.contains(*p))
        .map(|p| p.to_string())
        .collect();
    if !es_lectura && codigo.contains("ejecutar(") {
        encontrados.push("ejecutar( fuera de lectura".to_string());
    }
    if es_lectura {
        for literal in literales(codigo) {
            if !LITERALES_DE_LECTURA.contains(&literal.as_str()) {
                encontrados.push(format!("literal {literal:?}"));
            }
        }
    }
    encontrados
}

#[test]
fn los_modulos_de_la_simulacion_no_contienen_ninguna_forma_de_escribir() {
    for nombre in MODULOS_DE_SIMULACION {
        let texto = std::fs::read_to_string(fuente(nombre))
            .unwrap_or_else(|e| panic!("leer {nombre}: {e}"));
        let h = hallazgos(&sin_comentarios(&texto), nombre == "lectura.rs");
        assert!(h.is_empty(), "{nombre} contiene {h:?}");
    }
}

#[test]
fn el_escaneo_detectaria_una_violacion() {
    let escritura = format!("std::{}(raiz.join(\"cache\"), b\"\");", "fs::write");
    assert_eq!(hallazgos(&escritura, false), ["fs::write"]);

    let git_que_escribe = format!("git.ejecutar(raiz, [\"{}\", \"-m\", \"x\"])", "commit");
    let h = hallazgos(&git_que_escribe, true);
    assert!(h.contains(&"literal \"commit\"".to_string()), "{h:?}");
    assert!(h.contains(&"literal \"-m\"".to_string()), "{h:?}");

    assert_eq!(
        hallazgos(
            "lectura::blob(git, raiz, &id)?; git.ejecutar(raiz, [])",
            false
        ),
        ["ejecutar( fuera de lectura"]
    );

    // Lo legítimo no salta, y un '"' entre comillas simples no abre una cadena.
    let legitimo =
        "git.ejecutar(raiz, [\"cat-file\", \"blob\", id.as_str()]); let c = '\"'; fs::read(x)";
    assert!(
        hallazgos(legitimo, true).is_empty(),
        "{:?}",
        hallazgos(legitimo, true)
    );
}
