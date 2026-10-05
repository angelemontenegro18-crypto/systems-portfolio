//! El remediador: evalúa, arma la vuelta atrás, aplica, comprueba y, si algo
//! falla, revierte.
//!
//! El orden es la garantía:
//!
//! 1. se evalúan las puertas, sin efectos;
//! 2. si las otras tres pasan, se **arma** la vuelta atrás y se **verifica**;
//! 3. solo con las cuatro puertas en verde, se **aplica**;
//! 4. se **comprueba** el servicio después de aplicar;
//! 5. si aplicar o comprobar fallan, se **revierte**.
//!
//! Cualquier otro desenlace es un [`Ticket`].

use crate::accion::Accion;
use crate::politica::{Evaluacion, Politica, Propuesta, Veredicto};
use crate::ticket::{Estado, Ticket};

/// Lo que de verdad toca el sistema. Cada método devuelve `Err` con una
/// explicación si no pudo.
pub trait Actuador {
    /// Prepara la vuelta atrás: una instantánea de la caché, instancias en
    /// espera, el registro de las réplicas actuales.
    fn armar_reversion(&mut self, accion: &Accion) -> Result<(), String>;
    /// Comprueba que la vuelta atrás armada sirve.
    fn verificar_reversion(&mut self, accion: &Accion) -> Result<(), String>;
    /// Aplica la acción.
    fn aplicar(&mut self, accion: &Accion) -> Result<(), String>;
    /// Comprueba que el servicio quedó sano después de aplicar.
    fn comprobar(&mut self, accion: &Accion) -> Result<(), String>;
    /// Deshace la acción con la vuelta atrás armada.
    fn revertir(&mut self, accion: &Accion) -> Result<(), String>;
}

/// El resultado de atender una propuesta.
#[derive(Debug, Clone, PartialEq)]
pub enum Desenlace {
    /// Pasaron las cuatro puertas, se aplicó y se comprobó.
    Autoaplicada {
        /// La acción aplicada.
        accion: Accion,
        /// Las cuatro puertas, todas en verde.
        evaluacion: Evaluacion,
    },
    /// Va a una persona.
    Ticket(Ticket),
}

/// Una política y un actuador.
#[derive(Debug)]
pub struct Remediador<A> {
    politica: Politica,
    actuador: A,
}

impl<A: Actuador> Remediador<A> {
    /// El remediador con esta política y este actuador.
    pub fn nuevo(politica: Politica, actuador: A) -> Remediador<A> {
        Remediador { politica, actuador }
    }

    /// La política, solo para leer: atender propuestas no la cambia.
    pub fn politica(&self) -> &Politica {
        &self.politica
    }

    /// El actuador.
    pub fn actuador(&self) -> &A {
        &self.actuador
    }

    /// Atiende una propuesta: la autoaplica o la convierte en ticket.
    pub fn atender(&mut self, propuesta: &Propuesta) -> Desenlace {
        let accion = &propuesta.accion;
        let mut evaluacion = self.politica.evaluar(propuesta);
        if evaluacion.lista_para_armar() {
            evaluacion.reversibilidad = match self.armar_y_verificar(accion) {
                Ok(()) => {
                    Veredicto::Pasa(format!("armada y verificada: {}", accion.como_revertir()))
                }
                Err(detalle) => Veredicto::Falla(detalle),
            };
        }
        if !evaluacion.autoaplicable() {
            return Desenlace::Ticket(Ticket::nuevo(
                accion.clone(),
                propuesta.motivo.clone(),
                evaluacion,
                Estado::NoSeAplico,
            ));
        }

        let fallo = match self.actuador.aplicar(accion) {
            Err(e) => Some(format!("la aplicación falló: {e}")),
            Ok(()) => self
                .actuador
                .comprobar(accion)
                .err()
                .map(|e| format!("la comprobación posterior falló: {e}")),
        };
        let Some(fallo) = fallo else {
            return Desenlace::Autoaplicada {
                accion: accion.clone(),
                evaluacion,
            };
        };
        let estado = match self.actuador.revertir(accion) {
            Ok(()) => Estado::Revertida { fallo },
            Err(reversion) => Estado::ReversionFallida { fallo, reversion },
        };
        Desenlace::Ticket(Ticket::nuevo(
            accion.clone(),
            propuesta.motivo.clone(),
            evaluacion,
            estado,
        ))
    }

    fn armar_y_verificar(&mut self, accion: &Accion) -> Result<(), String> {
        self.actuador
            .armar_reversion(accion)
            .map_err(|e| format!("no se pudo armar la vuelta atrás: {e}"))?;
        self.actuador
            .verificar_reversion(accion)
            .map_err(|e| format!("la vuelta atrás armada no pasó la verificación: {e}"))
    }
}
