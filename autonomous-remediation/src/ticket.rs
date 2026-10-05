//! El ticket: lo que una persona necesita para decidir sin volver a
//! investigar.

use std::fmt;

use crate::accion::Accion;
use crate::politica::{Evaluacion, Puerta, Veredicto};

/// Qué pasó con la acción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Estado {
    /// Alguna puerta falló: no se tocó nada.
    NoSeAplico,
    /// Se aplicó, falló, y la vuelta atrás funcionó.
    Revertida {
        /// Qué falló.
        fallo: String,
    },
    /// Se aplicó, falló, y la vuelta atrás **también** falló.
    ReversionFallida {
        /// Qué falló al aplicar.
        fallo: String,
        /// Qué falló al revertir.
        reversion: String,
    },
}

/// Un caso para una persona: qué se iba a hacer, por qué, qué puerta falló y
/// con qué valores, qué pasó, y cómo revertir.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    accion: Accion,
    motivo: String,
    evaluacion: Evaluacion,
    estado: Estado,
}

impl Ticket {
    pub(crate) fn nuevo(
        accion: Accion,
        motivo: String,
        evaluacion: Evaluacion,
        estado: Estado,
    ) -> Ticket {
        Ticket {
            accion,
            motivo,
            evaluacion,
            estado,
        }
    }

    /// La acción propuesta.
    pub fn accion(&self) -> &Accion {
        &self.accion
    }

    /// Por qué se propuso.
    pub fn motivo(&self) -> &str {
        &self.motivo
    }

    /// Las cuatro puertas, con sus detalles.
    pub fn evaluacion(&self) -> &Evaluacion {
        &self.evaluacion
    }

    /// Qué pasó con la acción.
    pub fn estado(&self) -> &Estado {
        &self.estado
    }

    /// Las puertas que fallaron.
    pub fn puertas_fallidas(&self) -> Vec<Puerta> {
        self.evaluacion.fallidas()
    }

    /// `true` si la vuelta atrás falló: el servicio quedó en un estado que
    /// nadie eligió.
    pub fn requiere_atencion_inmediata(&self) -> bool {
        matches!(self.estado, Estado::ReversionFallida { .. })
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.requiere_atencion_inmediata() {
            writeln!(f, "TICKET URGENTE · {}", self.accion)?;
        } else {
            writeln!(f, "TICKET · {}", self.accion)?;
        }
        writeln!(f, "  Por qué:  {}", self.motivo)?;
        match &self.estado {
            Estado::NoSeAplico => writeln!(f, "  Estado:   no se aplicó")?,
            Estado::Revertida { fallo } => writeln!(f, "  Estado:   se aplicó y se revirtió — {fallo}")?,
            Estado::ReversionFallida { fallo, reversion } => {
                writeln!(f, "  Estado:   se aplicó, falló y NO se pudo revertir — {fallo}; al revertir: {reversion}")?
            }
        }
        writeln!(f, "  Puertas:")?;
        for puerta in Puerta::TODAS {
            let veredicto = self.evaluacion.veredicto(puerta);
            let marca = match veredicto {
                Veredicto::Pasa(_) => "✓",
                Veredicto::Falla(_) => "✗",
                Veredicto::Pendiente(_) => "·",
            };
            writeln!(
                f,
                "    {marca} {:<15} {}",
                puerta.nombre(),
                veredicto.detalle()
            )?;
        }
        write!(f, "  Cómo revertir: {}", self.accion.como_revertir())
    }
}
