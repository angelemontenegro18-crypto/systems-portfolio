//! Los tres niveles sobre un repositorio temporal: simular, preparar y aplicar,
//! con dos intentos de aplicar que se rechazan antes del bueno.
//!
//! `cargo run --example demo`. Necesita `git`; escribe solo en un directorio
//! temporal propio, que borra al terminar, y no lee la configuración de `git`
//! del usuario.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use staged_agent::{Agente, Confirmacion, Git};

/// Fecha fija para todos los commits: así los identificadores, la huella y la
/// salida de la demo son los mismos en cada corrida.
const FECHA: &str = "2026-01-01T12:00:00+00:00";

struct Demo {
    base: PathBuf,
    raiz: PathBuf,
    config_vacia: PathBuf,
}

impl Demo {
    fn git(&self, argumentos: &[&str]) -> String {
        let salida = Command::new("git")
            .arg("-C")
            .arg(&self.raiz)
            .args(["-c", "core.autocrlf=false"])
            .args(argumentos)
            .env("GIT_CONFIG_GLOBAL", &self.config_vacia)
            .env("GIT_CONFIG_SYSTEM", &self.config_vacia)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", FECHA)
            .env("GIT_COMMITTER_DATE", FECHA)
            .output()
            .expect("esta demo necesita `git` en el PATH");
        assert!(
            salida.status.success(),
            "git {argumentos:?}: {}",
            String::from_utf8_lossy(&salida.stderr)
        );
        String::from_utf8_lossy(&salida.stdout)
            .trim_end()
            .to_string()
    }

    fn escribir(&self, ruta: &str, contenido: &[u8]) {
        let destino = self.raiz.join(ruta);
        fs::create_dir_all(destino.parent().expect("tiene padre")).expect("crear directorios");
        fs::write(destino, contenido).expect("escribir");
    }

    fn agente(&self) -> Agente {
        let git = Git::del_sistema()
            .con_variable("GIT_CONFIG_GLOBAL", &self.config_vacia)
            .con_variable("GIT_CONFIG_SYSTEM", &self.config_vacia)
            .con_variable("GIT_CONFIG_NOSYSTEM", "1")
            .con_variable("GIT_AUTHOR_DATE", FECHA)
            .con_variable("GIT_COMMITTER_DATE", FECHA);
        Agente::nuevo(&self.raiz).con_git(git)
    }

    /// Resumen del árbol de trabajo, para mostrar que no cambió.
    fn huella_del_arbol(&self) -> String {
        fn recorrer(directorio: &Path, salida: &mut Vec<(String, Vec<u8>)>) {
            for entrada in fs::read_dir(directorio).expect("leer") {
                let ruta = entrada.expect("entrada").path();
                if ruta.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                if ruta.is_dir() {
                    recorrer(&ruta, salida);
                } else {
                    salida.push((
                        ruta.to_string_lossy().into_owned(),
                        fs::read(&ruta).expect("leer"),
                    ));
                }
            }
        }
        let mut archivos = Vec::new();
        recorrer(&self.raiz, &mut archivos);
        archivos.sort();
        let mut resumen = Sha256::new();
        for (ruta, contenido) in archivos {
            resumen.update(ruta.as_bytes());
            resumen.update(contenido);
        }
        resumen
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn main() {
    let base = std::env::temp_dir().join(format!("staged-agent-demo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let raiz = base.join("repo");
    fs::create_dir_all(&raiz).expect("crear el repositorio");
    let config_vacia = base.join("gitconfig-vacia");
    fs::write(&config_vacia, "").expect("crear la configuración vacía");
    let demo = Demo {
        base,
        raiz,
        config_vacia,
    };

    demo.git(&["init", "--quiet", "--initial-branch=main"]);
    demo.git(&["config", "user.name", "Demo"]);
    demo.git(&["config", "user.email", "demo@example.invalid"]);
    demo.git(&["config", "commit.gpgsign", "false"]);
    demo.escribir("README.md", b"# Proyecto\n\nTodo en orden.\n");
    demo.escribir(
        "docs/espacios.md",
        b"una linea con espacios   \notra con un tab\t\n",
    );
    demo.escribir("windows.txt", b"escrito\r\nen Windows\r\n");
    demo.escribir("sin_final.txt", b"falta el salto final");
    demo.escribir("logo.bin", b"\x89PNG\x00\x01");
    demo.git(&["add", "-A"]);
    demo.git(&["commit", "--quiet", "-m", "inicial"]);
    let agente = demo.agente();

    println!("staged-agent · demo sobre un repositorio temporal\n");
    println!("1 · Simular: calcular el plan sin escribir nada\n");
    let antes = demo.huella_del_arbol();
    let plan = agente.simular("main").expect("simular");
    for linea in plan.to_string().lines() {
        println!("   {linea}");
    }
    println!(
        "\n   ¿Cambió el árbol de trabajo? {}\n",
        si_no(demo.huella_del_arbol() != antes)
    );

    println!("2 · Preparar: aplicar el plan en un worktree aislado, sobre una rama nueva\n");
    let main_antes = demo.git(&["rev-parse", "main"]);
    let preparado = agente.preparar(&plan).expect("preparar");
    println!(
        "   rama {}  (commit {})",
        preparado.rama(),
        &preparado.commit()[..12]
    );
    println!(
        "   ¿Cambió el árbol de trabajo? {}",
        si_no(demo.huella_del_arbol() != antes)
    );
    println!(
        "   ¿Se movió main?              {}",
        si_no(demo.git(&["rev-parse", "main"]) != main_antes)
    );
    println!("   Para revisar: git diff main {}\n", preparado.rama());
    for linea in demo
        .git(&["diff", "--stat", "main", preparado.rama()])
        .lines()
    {
        println!("     {linea}");
    }

    println!("\n3 · Aplicar: solo el plan exacto, con confirmación\n");
    let huella = plan.huella();
    match agente.aplicar(&preparado, &huella, &Confirmacion::escrita("sí")) {
        Ok(_) => unreachable!("una confirmación genérica no alcanza"),
        Err(e) => println!("   con «sí»:\n     rechazado — {e}"),
    }
    demo.escribir("README.md", b"# Proyecto\n\nUn cambio a medio hacer.\n");
    let esperada = plan.confirmacion_esperada();
    match agente.aplicar(
        &preparado,
        &huella,
        &Confirmacion::escrita(esperada.as_str()),
    ) {
        Ok(_) => unreachable!("con el árbol sucio no se aplica"),
        Err(e) => println!("   con «{esperada}», pero con el árbol sucio:\n     rechazado — {e}"),
    }
    demo.git(&["checkout", "--quiet", "--", "README.md"]);
    let aplicado = agente
        .aplicar(
            &preparado,
            &huella,
            &Confirmacion::escrita(esperada.as_str()),
        )
        .expect("aplicar");
    println!(
        "   con «{esperada}» y el árbol limpio:\n     main avanza a {} ({} archivos)",
        &aplicado.commit[..12],
        aplicado.archivos
    );
    println!("\n   Simular de nuevo no propone nada:");
    for linea in agente.simular("main").expect("simular").to_string().lines() {
        println!("     {linea}");
    }
}

fn si_no(valor: bool) -> &'static str {
    if valor {
        "sí"
    } else {
        "no"
    }
}
