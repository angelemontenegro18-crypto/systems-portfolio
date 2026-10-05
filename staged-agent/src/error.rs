//! Los errores del agente.

use std::fmt;
use std::io;

use crate::git::ErrorGit;
use crate::rutas::ErrorRuta;

/// Por qué el agente se negó o no pudo.
///
/// Cada rechazo de [`crate::Agente::aplicar`] deja todo como estaba: las
/// comprobaciones van antes de tocar nada.
#[derive(Debug)]
pub enum Error {
    /// Falló una invocación de `git`, o no hay `git`.
    Git(ErrorGit),
    /// Falló una operación de archivos.
    Io(io::Error),
    /// Una ruta del plan no es segura.
    Ruta(ErrorRuta),
    /// El nombre de rama no es aceptable (vacío, empieza con `-`, tiene `..`…).
    NombreDeRamaInvalido(String),
    /// La rama no existe.
    RamaInexistente(String),
    /// Ya existe una rama con el nombre que se iba a crear; no se toca.
    RamaExistente(String),
    /// La rama destino ya no está donde estaba cuando se hizo el plan.
    DestinoMovido {
        /// Dónde estaba.
        esperado: String,
        /// Dónde está.
        actual: String,
    },
    /// La rama en uso no es la destino del plan.
    OtraRamaEnUso {
        /// La destino del plan.
        esperada: String,
        /// La que está en uso (`None` si `HEAD` no apunta a una rama).
        actual: Option<String>,
    },
    /// Un archivo no tenía, al prepararlo, el contenido que el plan esperaba.
    ContenidoInesperado(String),
    /// El commit preparado no reproduce el plan (un gancho o un filtro lo
    /// cambiaron); se deshizo.
    PreparacionInconsistente,
    /// La confirmación falta o no corresponde a este plan.
    SinConfirmacion,
    /// El árbol de trabajo tiene cambios sin commitear.
    ArbolSucio(Vec<String>),
    /// La rama preparada ya no reproduce la huella revisada.
    PlanAlterado,
    /// El plan no cambia nada: no hay qué preparar.
    PlanVacio,
    /// `git` respondió algo que no se esperaba.
    RespuestaInesperada(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Git(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "error de archivos: {e}"),
            Error::Ruta(e) => write!(f, "{e}"),
            Error::NombreDeRamaInvalido(n) => write!(f, "nombre de rama inválido: {n:?}"),
            Error::RamaInexistente(n) => write!(f, "la rama {n} no existe"),
            Error::RamaExistente(n) => write!(f, "la rama {n} ya existe; no se toca"),
            Error::DestinoMovido { esperado, actual } => write!(
                f,
                "la rama destino se movió: el plan es sobre {esperado} y ahora está en {actual}; hay que volver a simular"
            ),
            Error::OtraRamaEnUso { esperada, actual } => match actual {
                Some(a) => write!(f, "el plan es para {esperada}, pero la rama en uso es {a}"),
                None => write!(f, "el plan es para {esperada}, pero HEAD no apunta a ninguna rama"),
            },
            Error::ContenidoInesperado(r) => write!(f, "{r} no tiene el contenido que el plan esperaba"),
            Error::PreparacionInconsistente => {
                write!(f, "el commit preparado no reproduce el plan (¿un gancho o un filtro?); se deshizo")
            }
            Error::SinConfirmacion => write!(f, "falta la confirmación explícita de este plan"),
            Error::ArbolSucio(rutas) => write!(f, "el árbol de trabajo tiene cambios: {}", rutas.join(", ")),
            Error::PlanAlterado => write!(f, "la rama preparada no reproduce la huella revisada"),
            Error::PlanVacio => write!(f, "el plan no cambia nada"),
            Error::RespuestaInesperada(r) => write!(f, "respuesta inesperada de git: {r:?}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ErrorGit> for Error {
    fn from(e: ErrorGit) -> Error {
        Error::Git(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<ErrorRuta> for Error {
    fn from(e: ErrorRuta) -> Error {
        Error::Ruta(e)
    }
}
