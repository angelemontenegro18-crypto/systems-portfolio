//! Las acciones que una remediación puede proponer.
//!
//! Son tres, y ninguna toca la [envolvente](crate::envolvente): no hay una
//! acción para ampliar permisos, ni forma de expresarla.

use std::fmt;

/// Nombre de un servicio: de 1 a 63 caracteres entre minúsculas ASCII, dígitos
/// y `-`, sin `-` al principio ni al final.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Servicio(String);

impl Servicio {
    /// Valida el nombre.
    ///
    /// ```
    /// use autonomous_remediation::Servicio;
    ///
    /// assert!(Servicio::nuevo("api-pagos").is_some());
    /// assert!(Servicio::nuevo("API").is_none());
    /// assert!(Servicio::nuevo("-api").is_none());
    /// ```
    pub fn nuevo(nombre: &str) -> Option<Servicio> {
        let valido = (1..=63).contains(&nombre.len())
            && nombre
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !nombre.starts_with('-')
            && !nombre.ends_with('-');
        valido.then(|| Servicio(nombre.to_string()))
    }

    /// El nombre.
    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Servicio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Una remediación concreta sobre un servicio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accion {
    /// Reiniciar las instancias del servicio, de a una.
    Reiniciar {
        /// El servicio.
        servicio: Servicio,
    },
    /// Cambiar la cantidad de réplicas.
    EscalarReplicas {
        /// El servicio.
        servicio: Servicio,
        /// Las réplicas que tiene ahora: a esas se vuelve si hay que revertir.
        actuales: u32,
        /// Las réplicas que tendría.
        nuevas: u32,
    },
    /// Vaciar la caché del servicio.
    VaciarCache {
        /// El servicio.
        servicio: Servicio,
    },
}

/// El tipo de una acción, sin sus datos: lo que autoriza un permiso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TipoDeAccion {
    /// [`Accion::Reiniciar`].
    Reiniciar,
    /// [`Accion::EscalarReplicas`].
    EscalarReplicas,
    /// [`Accion::VaciarCache`].
    VaciarCache,
}

impl fmt::Display for TipoDeAccion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TipoDeAccion::Reiniciar => "reiniciar",
            TipoDeAccion::EscalarReplicas => "escalar réplicas",
            TipoDeAccion::VaciarCache => "vaciar la caché",
        })
    }
}

/// `1 réplica`, `3 réplicas`.
pub(crate) fn replicas(n: u32) -> String {
    if n == 1 {
        "1 réplica".to_string()
    } else {
        format!("{n} réplicas")
    }
}

impl Accion {
    /// El servicio sobre el que actúa.
    pub fn servicio(&self) -> &Servicio {
        match self {
            Accion::Reiniciar { servicio }
            | Accion::EscalarReplicas { servicio, .. }
            | Accion::VaciarCache { servicio } => servicio,
        }
    }

    /// El tipo de la acción.
    pub fn tipo(&self) -> TipoDeAccion {
        match self {
            Accion::Reiniciar { .. } => TipoDeAccion::Reiniciar,
            Accion::EscalarReplicas { .. } => TipoDeAccion::EscalarReplicas,
            Accion::VaciarCache { .. } => TipoDeAccion::VaciarCache,
        }
    }

    /// Cómo se deshace, en palabras. Va en el ticket, para que una persona
    /// pueda revertirla a mano.
    pub fn como_revertir(&self) -> String {
        match self {
            Accion::Reiniciar { servicio } => format!(
                "devolver el tráfico de `{servicio}` a las instancias en espera, que siguen con el estado anterior"
            ),
            Accion::EscalarReplicas { servicio, actuales, .. } => {
                format!("escalar `{servicio}` de vuelta a {}", replicas(*actuales))
            }
            Accion::VaciarCache { servicio } => {
                format!("restaurar la caché de `{servicio}` desde la instantánea tomada antes de vaciarla")
            }
        }
    }
}

impl fmt::Display for Accion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Accion::Reiniciar { servicio } => write!(f, "reiniciar `{servicio}`"),
            Accion::EscalarReplicas {
                servicio,
                actuales,
                nuevas,
            } => {
                write!(f, "escalar `{servicio}` de {actuales} a {nuevas} réplicas")
            }
            Accion::VaciarCache { servicio } => write!(f, "vaciar la caché de `{servicio}`"),
        }
    }
}
