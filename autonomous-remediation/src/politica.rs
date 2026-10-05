//! Las cuatro puertas.
//!
//! Una remediación se autoaplica **solo si pasan las cuatro**:
//!
//! 1. **Autoridad:** la acción está dentro de la [`Envolvente`].
//! 2. **Reversibilidad:** hay una vuelta atrás dentro de la envolvente, y se
//!    arma y se verifica **antes** de aplicar (eso lo hace el
//!    [`Remediador`](crate::Remediador)).
//! 3. **Beneficio neto acotado:** según el [modelo de costos](crate::costos),
//!    beneficio mayor que cero y daño colateral no mayor que un tope.
//! 4. **Confianza:** la señal que motivó la propuesta alcanza un mínimo.
//!
//! Es una conjunción, no un puntaje: una puerta muy holgada no compensa otra
//! que falla. Un dato ausente o inválido hace fallar su puerta: ante la duda,
//! ticket.
//!
//! Evaluar es una función pura (`&self`): no cambia la política ni la
//! envolvente, y la misma propuesta da siempre la misma evaluación.

use std::fmt;

use crate::accion::Accion;
use crate::costos::{simular, Metricas};
use crate::envolvente::Envolvente;

/// Los mínimos y máximos de la política. Los elige una persona.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Umbrales {
    confianza_minima: f64,
    dano_maximo: f64,
}

impl Umbrales {
    /// `confianza_minima` en `(0, 1]` y `dano_maximo` en `[0, 1]`; si no,
    /// `None`. Una confianza mínima de cero haría de la cuarta puerta una que
    /// siempre pasa.
    pub fn nuevos(confianza_minima: f64, dano_maximo: f64) -> Option<Umbrales> {
        let valido =
            confianza_minima > 0.0 && confianza_minima <= 1.0 && (0.0..=1.0).contains(&dano_maximo);
        valido.then_some(Umbrales {
            confianza_minima,
            dano_maximo,
        })
    }

    /// La confianza mínima de la señal.
    pub fn confianza_minima(&self) -> f64 {
        self.confianza_minima
    }

    /// El daño colateral máximo, en fracción de solicitudes.
    pub fn dano_maximo(&self) -> f64 {
        self.dano_maximo
    }
}

/// Lo que llega del monitoreo: qué hacer, por qué, y con qué evidencia.
#[derive(Debug, Clone, PartialEq)]
pub struct Propuesta {
    /// La acción propuesta.
    pub accion: Accion,
    /// Por qué: lo que se observó. Va tal cual al ticket.
    pub motivo: String,
    /// Calidad de la señal, en `[0, 1]`. `None` si no hay dato.
    pub confianza: Option<f64>,
    /// Las métricas para el modelo de costos. `None` si faltan.
    pub metricas: Option<Metricas>,
}

/// Una de las cuatro puertas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Puerta {
    /// La acción está dentro de la envolvente.
    Autoridad,
    /// Hay una vuelta atrás, armada y verificada antes de aplicar.
    Reversibilidad,
    /// Beneficio mayor que cero y daño colateral acotado.
    BeneficioNeto,
    /// La señal alcanza la confianza mínima.
    Confianza,
}

impl Puerta {
    /// Las cuatro, en orden.
    pub const TODAS: [Puerta; 4] = [
        Puerta::Autoridad,
        Puerta::Reversibilidad,
        Puerta::BeneficioNeto,
        Puerta::Confianza,
    ];

    /// Nombre para el ticket.
    pub fn nombre(&self) -> &'static str {
        match self {
            Puerta::Autoridad => "autoridad",
            Puerta::Reversibilidad => "reversibilidad",
            Puerta::BeneficioNeto => "beneficio neto",
            Puerta::Confianza => "confianza",
        }
    }
}

impl fmt::Display for Puerta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nombre())
    }
}

/// El resultado de una puerta, con el detalle y los valores que lo explican.
#[derive(Debug, Clone, PartialEq)]
pub enum Veredicto {
    /// Pasa.
    Pasa(String),
    /// Falla.
    Falla(String),
    /// Todavía no se sabe: la vuelta atrás es posible, pero solo se arma si
    /// las otras tres puertas pasan.
    Pendiente(String),
}

impl Veredicto {
    /// `true` solo si pasa.
    pub fn pasa(&self) -> bool {
        matches!(self, Veredicto::Pasa(_))
    }

    /// `true` solo si falla.
    pub fn falla(&self) -> bool {
        matches!(self, Veredicto::Falla(_))
    }

    /// La explicación, con sus valores.
    pub fn detalle(&self) -> &str {
        match self {
            Veredicto::Pasa(d) | Veredicto::Falla(d) | Veredicto::Pendiente(d) => d,
        }
    }
}

/// Las cuatro puertas evaluadas.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluacion {
    pub(crate) autoridad: Veredicto,
    pub(crate) reversibilidad: Veredicto,
    pub(crate) beneficio_neto: Veredicto,
    pub(crate) confianza: Veredicto,
}

impl Evaluacion {
    /// El veredicto de una puerta.
    pub fn veredicto(&self, puerta: Puerta) -> &Veredicto {
        match puerta {
            Puerta::Autoridad => &self.autoridad,
            Puerta::Reversibilidad => &self.reversibilidad,
            Puerta::BeneficioNeto => &self.beneficio_neto,
            Puerta::Confianza => &self.confianza,
        }
    }

    /// Las cuatro pasan. Conjunción: ni mayoría ni suma.
    pub fn autoaplicable(&self) -> bool {
        self.autoridad.pasa()
            && self.reversibilidad.pasa()
            && self.beneficio_neto.pasa()
            && self.confianza.pasa()
    }

    /// Las puertas que fallaron, en orden.
    pub fn fallidas(&self) -> Vec<Puerta> {
        Puerta::TODAS
            .into_iter()
            .filter(|&p| self.veredicto(p).falla())
            .collect()
    }

    /// Las otras tres pasan y la vuelta atrás es posible: corresponde armarla.
    pub(crate) fn lista_para_armar(&self) -> bool {
        self.autoridad.pasa()
            && self.beneficio_neto.pasa()
            && self.confianza.pasa()
            && matches!(self.reversibilidad, Veredicto::Pendiente(_))
    }
}

/// La envolvente y los umbrales. Se fijan al construirla y no cambian.
#[derive(Debug, Clone, PartialEq)]
pub struct Politica {
    envolvente: Envolvente,
    umbrales: Umbrales,
}

impl Politica {
    /// La política con esta envolvente y estos umbrales.
    pub fn nueva(envolvente: Envolvente, umbrales: Umbrales) -> Politica {
        Politica {
            envolvente,
            umbrales,
        }
    }

    /// La envolvente, solo para leer.
    pub fn envolvente(&self) -> &Envolvente {
        &self.envolvente
    }

    /// Los umbrales.
    pub fn umbrales(&self) -> &Umbrales {
        &self.umbrales
    }

    /// Evalúa las cuatro puertas. La reversibilidad queda
    /// [`Pendiente`](Veredicto::Pendiente) si es posible: armarla es un efecto,
    /// y lo hace el [`Remediador`](crate::Remediador).
    pub fn evaluar(&self, propuesta: &Propuesta) -> Evaluacion {
        let accion = &propuesta.accion;
        let autoridad = match self.envolvente.admite(accion) {
            Ok(detalle) => Veredicto::Pasa(detalle),
            Err(detalle) => Veredicto::Falla(detalle),
        };
        let reversibilidad = match self.envolvente.admite_reversion(accion) {
            Ok(como) => Veredicto::Pendiente(format!(
                "posible ({como}); se arma solo si las otras tres pasan"
            )),
            Err(detalle) => Veredicto::Falla(detalle),
        };
        Evaluacion {
            autoridad,
            reversibilidad,
            beneficio_neto: self.beneficio_neto(accion, propuesta.metricas.as_ref()),
            confianza: self.confianza(propuesta.confianza),
        }
    }

    fn beneficio_neto(&self, accion: &Accion, metricas: Option<&Metricas>) -> Veredicto {
        let Some(metricas) = metricas else {
            return Veredicto::Falla("sin métricas no hay simulación".to_string());
        };
        let simulacion = match simular(accion, metricas) {
            Ok(s) => s,
            Err(detalle) => return Veredicto::Falla(detalle),
        };
        let tope = self.umbrales.dano_maximo;
        let (beneficio, dano) = (simulacion.beneficio, simulacion.dano);
        // Comparaciones directas: con un NaN, las dos dan `false` y la puerta falla.
        let hay_beneficio = beneficio > 0.0;
        let dano_acotado = dano <= tope;
        if hay_beneficio && dano_acotado {
            return Veredicto::Pasa(format!(
                "beneficio {beneficio:.2} > 0 y daño colateral {dano:.2} ≤ tope {tope:.2}"
            ));
        }
        let mut motivos = Vec::new();
        if !hay_beneficio {
            motivos.push(format!("beneficio {beneficio:.2}, no mayor que 0"));
        }
        if !dano_acotado {
            motivos.push(format!("daño colateral {dano:.2} > tope {tope:.2}"));
        }
        Veredicto::Falla(motivos.join("; "))
    }

    fn confianza(&self, confianza: Option<f64>) -> Veredicto {
        let minima = self.umbrales.confianza_minima;
        match confianza {
            None => Veredicto::Falla("sin dato de confianza".to_string()),
            Some(c) if !(0.0..=1.0).contains(&c) => {
                Veredicto::Falla(format!("confianza {c} fuera de [0, 1]"))
            }
            Some(c) if c >= minima => Veredicto::Pasa(format!("{c:.2} ≥ mínimo {minima:.2}")),
            Some(c) => Veredicto::Falla(format!("{c:.2} < mínimo {minima:.2}")),
        }
    }
}
