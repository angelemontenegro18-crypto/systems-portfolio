//! Repositorios de prueba temporales y herméticos.
//!
//! Cada prueba arma su propio repositorio bajo `std::env::temp_dir()` y lo
//! borra al terminar. La configuración de `git` del usuario y del sistema no
//! cuenta: `GIT_CONFIG_GLOBAL` y `GIT_CONFIG_SYSTEM` apuntan a un archivo vacío
//! propio, los ganchos a un directorio vacío, las variables de identidad
//! heredadas se quitan, y la identidad, la firma y los finales de línea se fijan
//! en la configuración local del repositorio. Sin red.
//!
//! Si no hay `git`, las pruebas fallan con un mensaje claro: no se omiten.

#![allow(dead_code)] // cada archivo de pruebas usa una parte

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest, Sha256};
use staged_agent::{Agente, Git};

/// Variables que podrían colar una identidad o una configuración ajena.
const VARIABLES_AJENAS: [&str; 7] = [
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "EMAIL",
    "GIT_DIR",
    "GIT_WORK_TREE",
];

pub struct Repo {
    pub raiz: PathBuf,
    base: PathBuf,
    config_vacia: PathBuf,
}

/// Falla con un mensaje claro si no hay `git`.
pub fn exigir_git() {
    match Command::new("git").arg("--version").output() {
        Ok(salida) if salida.status.success() => {}
        Ok(salida) => panic!("`git --version` terminó con {}: estas pruebas necesitan git", salida.status),
        Err(e) => panic!("no se pudo ejecutar `git` ({e}): estas pruebas necesitan git en el PATH y no se omiten"),
    }
}

impl Repo {
    /// Un repositorio vacío, con la rama `main`.
    pub fn nuevo(nombre: &str) -> Repo {
        exigir_git();
        static CONTADOR: AtomicUsize = AtomicUsize::new(0);
        let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "staged-agent-prueba-{nombre}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let raiz = base.join("repo");
        let ganchos = base.join("ganchos-vacios");
        fs::create_dir_all(&raiz).expect("crear el directorio del repositorio");
        fs::create_dir_all(&ganchos).expect("crear el directorio de ganchos");
        let config_vacia = base.join("gitconfig-vacia");
        fs::write(&config_vacia, "").expect("crear la configuración vacía");

        let repo = Repo {
            raiz,
            base,
            config_vacia,
        };
        repo.git(["init", "--quiet", "--initial-branch=main"]);
        repo.git(["config", "user.name", "Prueba"]);
        repo.git(["config", "user.email", "prueba@example.invalid"]);
        repo.git(["config", "commit.gpgsign", "false"]);
        repo.git(["config", "core.autocrlf", "false"]);
        repo.git([
            OsStr::new("config"),
            OsStr::new("core.hooksPath"),
            ganchos.as_os_str(),
        ]);
        repo
    }

    /// Un directorio propio de la prueba, fuera del repositorio.
    pub fn directorio_aparte(&self, nombre: &str) -> PathBuf {
        let ruta = self.base.join(nombre);
        fs::create_dir_all(&ruta).expect("crear el directorio aparte");
        ruta
    }

    fn comando(&self, directorio: &Path) -> Command {
        let mut comando = Command::new("git");
        for variable in VARIABLES_AJENAS {
            comando.env_remove(variable);
        }
        comando
            .arg("-C")
            .arg(directorio)
            .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
            .env("GIT_CONFIG_GLOBAL", &self.config_vacia)
            .env("GIT_CONFIG_SYSTEM", &self.config_vacia)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        comando
    }

    /// Corre `git` en el repositorio; falla la prueba si `git` falla.
    pub fn git<I, S>(&self, argumentos: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.git_en(&self.raiz.clone(), argumentos)
    }

    /// Corre `git` en otro directorio (un worktree, por ejemplo).
    pub fn git_en<I, S>(&self, directorio: &Path, argumentos: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let argumentos: Vec<_> = argumentos
            .into_iter()
            .map(|a| a.as_ref().to_os_string())
            .collect();
        let salida = self
            .comando(directorio)
            .args(&argumentos)
            .output()
            .expect("ejecutar git");
        assert!(
            salida.status.success(),
            "git {argumentos:?} falló: {}",
            String::from_utf8_lossy(&salida.stderr)
        );
        String::from_utf8_lossy(&salida.stdout).trim().to_string()
    }

    /// Bytes de un archivo de un commit.
    pub fn mostrar(&self, revision: &str, ruta: &str) -> Vec<u8> {
        let salida = self
            .comando(&self.raiz)
            .args(["show", &format!("{revision}:{ruta}")])
            .output()
            .expect("git show");
        assert!(salida.status.success(), "git show {revision}:{ruta} falló");
        salida.stdout
    }

    pub fn escribir(&self, ruta: &str, contenido: &[u8]) {
        let destino = self.raiz.join(ruta);
        if let Some(padre) = destino.parent() {
            fs::create_dir_all(padre).expect("crear directorios");
        }
        fs::write(destino, contenido).expect("escribir el archivo");
    }

    pub fn leer(&self, ruta: &str) -> Vec<u8> {
        fs::read(self.raiz.join(ruta)).expect("leer el archivo")
    }

    /// Commitea todo y devuelve el commit.
    pub fn commit_todo(&self, mensaje: &str) -> String {
        self.git(["add", "-A"]);
        self.git(["commit", "--quiet", "-m", mensaje]);
        self.git(["rev-parse", "HEAD"])
    }

    pub fn punta(&self, rama: &str) -> String {
        self.git(["rev-parse", &format!("refs/heads/{rama}")])
    }

    pub fn ramas(&self) -> Vec<String> {
        let salida = self.git(["for-each-ref", "--format=%(refname)", "refs/heads"]);
        salida.lines().map(str::to_string).collect()
    }

    pub fn cantidad_de_worktrees(&self) -> usize {
        self.git(["worktree", "list", "--porcelain"])
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count()
    }

    pub fn estado(&self) -> String {
        self.git(["status", "--porcelain", "--untracked-files=all"])
    }

    /// `git` para el agente, con el mismo aislamiento que las pruebas.
    pub fn git_aislado(&self) -> Git {
        let mut git = Git::del_sistema()
            .con_variable("GIT_CONFIG_GLOBAL", &self.config_vacia)
            .con_variable("GIT_CONFIG_SYSTEM", &self.config_vacia)
            .con_variable("GIT_CONFIG_NOSYSTEM", "1");
        for variable in VARIABLES_AJENAS {
            git = git.sin_variable(variable);
        }
        git
    }

    pub fn agente(&self) -> Agente {
        Agente::nuevo(&self.raiz).con_git(self.git_aislado())
    }

    /// Resumen de todo lo que hay bajo la raíz, `.git` incluido: rutas, tipos,
    /// tamaños, fechas de modificación y contenidos. Si algo se escribe —aunque
    /// sea con el mismo contenido—, cambia.
    pub fn huella_completa(&self) -> String {
        resumen_de(&self.raiz, true)
    }

    /// Lo mismo, sin `.git`: el árbol de trabajo.
    pub fn huella_del_arbol(&self) -> String {
        resumen_de(&self.raiz, false)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn resumen_de(raiz: &Path, con_git: bool) -> String {
    let mut entradas = Vec::new();
    recorrer(raiz, raiz, con_git, &mut entradas);
    entradas.sort();
    let mut resumen = Sha256::new();
    for entrada in entradas {
        resumen.update(entrada);
    }
    resumen
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn recorrer(raiz: &Path, directorio: &Path, con_git: bool, salida: &mut Vec<Vec<u8>>) {
    for entrada in fs::read_dir(directorio).expect("leer el directorio") {
        let ruta = entrada.expect("entrada").path();
        let relativa = ruta
            .strip_prefix(raiz)
            .expect("bajo la raíz")
            .to_string_lossy()
            .into_owned();
        if !con_git && relativa == ".git" {
            continue;
        }
        let metadatos = fs::symlink_metadata(&ruta).expect("metadatos");
        let modificado = metadatos
            .modified()
            .map(|t| format!("{t:?}"))
            .unwrap_or_default();
        let mut registro = format!(
            "{relativa}|{:?}|{}|{modificado}|",
            metadatos.file_type(),
            metadatos.len()
        )
        .into_bytes();
        if metadatos.is_file() {
            registro.extend(Sha256::digest(fs::read(&ruta).expect("leer")));
        }
        salida.push(registro);
        if metadatos.is_dir() {
            recorrer(raiz, &ruta, con_git, salida);
        }
    }
}
