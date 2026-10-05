//! La envolvente: lo que la remediación puede hacer por su cuenta.
//!
//! La arma una persona y se entrega a la [`Politica`](crate::Politica) al
//! construirla. Desde ahí no cambia: la política solo la expone como `&`, los
//! métodos que la arman consumen la envolvente por valor, y ninguna
//! [`Accion`] la toca. Lo que se propone se compara contra ella; una propuesta
//! no trae su propia autorización.
//!
//! ```compile_fail,E0507
//! use autonomous_remediation::{Envolvente, Permiso, Politica, Servicio, Umbrales};
//!
//! let umbrales = Umbrales::nuevos(0.9, 0.3).unwrap();
//! let politica = Politica::nueva(Envolvente::vacia(), umbrales);
//! let api = Servicio::nuevo("api").unwrap();
//! // error[E0507]: armar la envolvente la consume, y la política solo la presta.
//! let ampliada = politica.envolvente().con(Permiso::reiniciar(api));
//! ```

use crate::accion::{replicas, Accion, Servicio, TipoDeAccion};

/// Un permiso: un tipo de acción sobre un servicio y, para escalar, el rango
/// de réplicas admitido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permiso {
    servicio: Servicio,
    tipo: TipoDeAccion,
    replicas: Option<(u32, u32)>,
}

impl Permiso {
    /// Reiniciar el servicio.
    pub fn reiniciar(servicio: Servicio) -> Permiso {
        Permiso {
            servicio,
            tipo: TipoDeAccion::Reiniciar,
            replicas: None,
        }
    }

    /// Vaciar la caché del servicio.
    pub fn vaciar_cache(servicio: Servicio) -> Permiso {
        Permiso {
            servicio,
            tipo: TipoDeAccion::VaciarCache,
            replicas: None,
        }
    }

    /// Escalar el servicio dentro de `[minimo, maximo]` réplicas. `None` si el
    /// rango está vacío o incluye el cero: dejar un servicio sin réplicas no es
    /// una remediación.
    pub fn escalar(servicio: Servicio, minimo: u32, maximo: u32) -> Option<Permiso> {
        (minimo >= 1 && minimo <= maximo).then_some(Permiso {
            servicio,
            tipo: TipoDeAccion::EscalarReplicas,
            replicas: Some((minimo, maximo)),
        })
    }
}

/// El conjunto de permisos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Envolvente {
    permisos: Vec<Permiso>,
}

impl Envolvente {
    /// Una envolvente sin permisos: todo va a ticket.
    pub fn vacia() -> Envolvente {
        Envolvente::default()
    }

    /// Agrega un permiso. Consume la envolvente: se arma antes de entregarla.
    pub fn con(mut self, permiso: Permiso) -> Envolvente {
        self.permisos.push(permiso);
        self
    }

    /// Los permisos.
    pub fn permisos(&self) -> &[Permiso] {
        &self.permisos
    }

    /// Los permisos para este tipo de acción sobre este servicio.
    fn para<'a>(&'a self, accion: &'a Accion) -> impl Iterator<Item = &'a Permiso> + 'a {
        self.permisos
            .iter()
            .filter(move |p| p.servicio == *accion.servicio() && p.tipo == accion.tipo())
    }

    /// ¿Está `accion` dentro? `Ok` o `Err` con la explicación.
    pub fn admite(&self, accion: &Accion) -> Result<String, String> {
        let mut permisos = self.para(accion).peekable();
        if permisos.peek().is_none() {
            return Err(format!(
                "la envolvente no permite {} sobre `{}`",
                accion.tipo(),
                accion.servicio()
            ));
        }
        match accion {
            Accion::EscalarReplicas { nuevas, .. } => {
                let rangos: Vec<(u32, u32)> = permisos.filter_map(|p| p.replicas).collect();
                match rangos
                    .iter()
                    .find(|(min, max)| (min..=max).contains(&nuevas))
                {
                    Some((min, max)) => Ok(format!(
                        "escalar a {} está dentro de [{min}, {max}]",
                        replicas(*nuevas)
                    )),
                    None => Err(format!(
                        "escalar a {} queda fuera de {}",
                        replicas(*nuevas),
                        describir(&rangos)
                    )),
                }
            }
            _ => Ok(format!("{accion}: permitido")),
        }
    }

    /// ¿La vuelta atrás de `accion` también está dentro? Para escalar, volver a
    /// las réplicas actuales tiene que estar en el rango; reiniciar y vaciar la
    /// caché se revierten dentro del mismo permiso.
    pub fn admite_reversion(&self, accion: &Accion) -> Result<String, String> {
        match accion {
            Accion::EscalarReplicas { actuales, .. } => {
                let rangos: Vec<(u32, u32)> =
                    self.para(accion).filter_map(|p| p.replicas).collect();
                if rangos
                    .iter()
                    .any(|(min, max)| (min..=max).contains(&actuales))
                {
                    Ok(accion.como_revertir())
                } else {
                    Err(format!(
                        "volver a {} queda fuera de {}: no habría cómo revertir",
                        replicas(*actuales),
                        describir(&rangos)
                    ))
                }
            }
            _ => Ok(accion.como_revertir()),
        }
    }
}

fn describir(rangos: &[(u32, u32)]) -> String {
    if rangos.is_empty() {
        return "la envolvente".to_string();
    }
    rangos
        .iter()
        .map(|(min, max)| format!("[{min}, {max}]"))
        .collect::<Vec<_>>()
        .join(" ∪ ")
}
