//! Rutas relativas que no pueden salir de la raíz.
//!
//! Dos defensas, en dos momentos:
//!
//! - **Al construir** una [`RutaRelativa`]: nada de `..`, `.`, componentes
//!   vacíos, rutas absolutas, unidades de Windows, `\` ni `:`.
//! - **Al tocar el disco**, [`resolver_dentro`]: ningún componente puede ser un
//!   enlace simbólico, y la ruta canónica tiene que quedar dentro de la raíz
//!   canónica. Un enlace que apunta afuera —o adentro, a otro archivo— haría
//!   que el agente escriba algo distinto de lo que dice el plan.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Ruta relativa a la raíz del repositorio, con `/` como separador.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RutaRelativa(String);

impl RutaRelativa {
    /// Valida `texto` como ruta relativa segura.
    ///
    /// ```
    /// use staged_agent::rutas::RutaRelativa;
    ///
    /// assert!(RutaRelativa::nueva("docs/guia.md").is_ok());
    /// assert!(RutaRelativa::nueva("../afuera.txt").is_err());
    /// assert!(RutaRelativa::nueva("/etc/hosts").is_err());
    /// ```
    pub fn nueva(texto: &str) -> Result<RutaRelativa, ErrorRuta> {
        let rechazo = |motivo| {
            Err(ErrorRuta::Invalida {
                ruta: texto.to_string(),
                motivo,
            })
        };
        if texto.is_empty() {
            return rechazo("está vacía");
        }
        if texto.starts_with('/') {
            return rechazo("es absoluta");
        }
        if texto.contains('\\') {
            return rechazo("usa `\\`, que en Windows es un separador");
        }
        if texto.contains(':') {
            return rechazo("usa `:`, que en Windows indica una unidad o un flujo alternativo");
        }
        if texto.contains('\0') {
            return rechazo("contiene un byte nulo");
        }
        for componente in texto.split('/') {
            match componente {
                "" => return rechazo("tiene un componente vacío"),
                "." | ".." => return rechazo("tiene un componente `.` o `..`"),
                _ => {}
            }
        }
        Ok(RutaRelativa(texto.to_string()))
    }

    /// La ruta como texto, con `/`.
    pub fn como_str(&self) -> &str {
        &self.0
    }

    /// Los componentes, en orden.
    pub fn componentes(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl fmt::Display for RutaRelativa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Por qué una ruta no es aceptable.
#[derive(Debug)]
pub enum ErrorRuta {
    /// La ruta no pasa la validación de [`RutaRelativa::nueva`].
    Invalida {
        /// La ruta rechazada.
        ruta: String,
        /// Por qué.
        motivo: &'static str,
    },
    /// Un componente es un enlace simbólico.
    EnlaceSimbolico(RutaRelativa),
    /// La ruta canónica cae fuera de la raíz.
    FueraDeLaRaiz(RutaRelativa),
    /// La ruta no existe en el disco.
    NoExiste(RutaRelativa),
    /// Un error de E/S al inspeccionarla.
    Io(io::Error),
}

impl fmt::Display for ErrorRuta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorRuta::Invalida { ruta, motivo } => write!(f, "ruta inválida {ruta:?}: {motivo}"),
            ErrorRuta::EnlaceSimbolico(r) => write!(f, "{r} pasa por un enlace simbólico"),
            ErrorRuta::FueraDeLaRaiz(r) => write!(f, "{r} queda fuera de la raíz"),
            ErrorRuta::NoExiste(r) => write!(f, "{r} no existe"),
            ErrorRuta::Io(e) => write!(f, "no se pudo inspeccionar la ruta: {e}"),
        }
    }
}

impl std::error::Error for ErrorRuta {}

/// Dónde está `ruta` dentro de `raiz`, comprobando que no se salga.
///
/// Recorre la ruta componente por componente y rechaza cualquier enlace
/// simbólico, aunque apunte adentro. Después exige que la forma canónica sea
/// exactamente la ruta recorrida y que quede dentro de la raíz canónica. Esa
/// segunda barrera cubre lo que el sistema de archivos resuelva distinto de como
/// se escribió la ruta: en uno que no distingue mayúsculas, `A.txt` y `a.txt`
/// son el mismo archivo, y el plan diría uno y se escribiría el otro.
///
/// La ruta tiene que existir: el agente solo modifica archivos que ya están.
pub fn resolver_dentro(raiz: &Path, ruta: &RutaRelativa) -> Result<PathBuf, ErrorRuta> {
    let raiz_canonica = raiz.canonicalize().map_err(ErrorRuta::Io)?;
    let mut actual = raiz_canonica.clone();
    for componente in ruta.componentes() {
        actual.push(componente);
        match fs::symlink_metadata(&actual) {
            Ok(metadatos) if metadatos.file_type().is_symlink() => {
                return Err(ErrorRuta::EnlaceSimbolico(ruta.clone()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ErrorRuta::NoExiste(ruta.clone()))
            }
            Err(e) => return Err(ErrorRuta::Io(e)),
        }
    }
    let canonica = actual.canonicalize().map_err(ErrorRuta::Io)?;
    if canonica != actual || !canonica.starts_with(&raiz_canonica) {
        return Err(ErrorRuta::FueraDeLaRaiz(ruta.clone()));
    }
    Ok(canonica)
}
