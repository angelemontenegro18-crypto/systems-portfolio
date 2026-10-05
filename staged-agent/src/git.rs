//! El `git` del sistema, sin shell y con argumentos fijos.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Cómo invocar a `git`.
///
/// Cada invocación es `git -C <raíz> -c core.autocrlf=false <argumentos>`:
/// sin shell, con la entrada estándar cerrada y sin preguntas por terminal. El
/// `core.autocrlf=false` es deliberado: el agente trabaja con los bytes
/// exactos, y una conversión de finales de línea a sus espaldas cambiaría lo
/// que se revisó.
#[derive(Debug, Clone)]
pub struct Git {
    programa: OsString,
    entorno: Vec<(OsString, OsString)>,
    quitadas: Vec<OsString>,
}

impl Git {
    /// El `git` que esté en el `PATH`.
    pub fn del_sistema() -> Git {
        Git {
            programa: OsString::from("git"),
            entorno: Vec::new(),
            quitadas: Vec::new(),
        }
    }

    /// Agrega una variable de entorno a cada invocación; por ejemplo, para
    /// aislar la configuración en las pruebas con `GIT_CONFIG_GLOBAL`.
    pub fn con_variable(mut self, nombre: impl Into<OsString>, valor: impl Into<OsString>) -> Git {
        self.entorno.push((nombre.into(), valor.into()));
        self
    }

    /// Quita una variable de entorno en cada invocación; por ejemplo, una
    /// identidad `GIT_AUTHOR_NAME` heredada que haría distinta una prueba.
    pub fn sin_variable(mut self, nombre: impl Into<OsString>) -> Git {
        self.quitadas.push(nombre.into());
        self
    }

    /// Ejecuta `git` en `raiz` y devuelve su salida estándar.
    pub(crate) fn ejecutar<I, S>(&self, raiz: &Path, argumentos: I) -> Result<Vec<u8>, ErrorGit>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let argumentos: Vec<OsString> = argumentos
            .into_iter()
            .map(|a| a.as_ref().to_os_string())
            .collect();
        let mut comando = Command::new(&self.programa);
        for nombre in &self.quitadas {
            comando.env_remove(nombre);
        }
        let salida = comando
            .arg("-C")
            .arg(raiz)
            .args(["-c", "core.autocrlf=false"])
            .args(&argumentos)
            .envs(self.entorno.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    ErrorGit::NoEncontrado
                } else {
                    ErrorGit::NoSePudoEjecutar(e)
                }
            })?;
        if salida.status.success() {
            Ok(salida.stdout)
        } else {
            Err(ErrorGit::Fallo {
                argumentos: argumentos
                    .iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                codigo: salida.status.code(),
                mensaje: String::from_utf8_lossy(&salida.stderr).trim().to_string(),
            })
        }
    }
}

/// Un fallo al invocar `git`.
#[derive(Debug)]
pub enum ErrorGit {
    /// No hay un `git` en el `PATH`.
    NoEncontrado,
    /// El proceso no se pudo lanzar.
    NoSePudoEjecutar(io::Error),
    /// `git` terminó con error.
    Fallo {
        /// Los argumentos, para el diagnóstico.
        argumentos: String,
        /// El código de salida, si lo hubo.
        codigo: Option<i32>,
        /// Lo que `git` escribió en su salida de errores.
        mensaje: String,
    },
}

impl fmt::Display for ErrorGit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorGit::NoEncontrado => write!(f, "no se encontró `git` en el PATH"),
            ErrorGit::NoSePudoEjecutar(e) => write!(f, "no se pudo ejecutar `git`: {e}"),
            ErrorGit::Fallo {
                argumentos,
                codigo,
                mensaje,
            } => {
                write!(f, "`git {argumentos}` falló")?;
                if let Some(c) = codigo {
                    write!(f, " (código {c})")?;
                }
                if !mensaje.is_empty() {
                    write!(f, ": {mensaje}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ErrorGit {}
