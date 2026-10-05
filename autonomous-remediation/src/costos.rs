//! Un modelo de costos **ilustrativo y sin calibrar**.
//!
//! Mide todo en la misma unidad, la fracción de solicitudes afectadas: el
//! beneficio es la que deja de fallar o de rechazarse, y el daño colateral la
//! que se degrada por la acción misma. Las fórmulas son supuestos de juguete,
//! elegidos para que las demos se lean solas; un sistema real los mediría.
//!
//! - **Reiniciar:** supone que los errores vienen de instancias trabadas y que
//!   el reinicio los elimina todos; mientras se reinicia, falta una réplica de
//!   cada `replicas`.
//! - **Escalar:** por encima de la capacidad (`utilizacion > 1`) se rechaza la
//!   fracción `1 − 1/u` de las solicitudes. El beneficio es cuánto baja ese
//!   rechazo; el daño, cuánto sube.
//! - **Vaciar la caché:** supone que los errores vienen de entradas viejas;
//!   mientras se vuelve a llenar, las lecturas que resolvía la caché caen sobre
//!   el origen, y dañan en proporción a lo cargado que está.

use crate::accion::Accion;

/// Lo que se sabe del servicio. Todos los valores tienen que ser finitos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metricas {
    /// Fracción de solicitudes que fallan, en `[0, 1]`.
    pub tasa_de_error: f64,
    /// Carga sobre capacidad: 1 es el límite. No negativa.
    pub utilizacion: f64,
    /// Fracción de lecturas que resuelve la caché, en `[0, 1]`.
    pub aciertos_de_cache: f64,
    /// Réplicas sanas ahora.
    pub replicas: u32,
}

/// Lo que el modelo estima que haría una acción.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Simulacion {
    /// Fracción de solicitudes que dejarían de fallar o de rechazarse.
    pub beneficio: f64,
    /// Fracción de solicitudes que la acción misma degradaría.
    pub dano: f64,
}

/// Fracción de solicitudes rechazadas con utilización `u`.
fn rechazo(u: f64) -> f64 {
    if u > 1.0 {
        1.0 - 1.0 / u
    } else {
        0.0
    }
}

fn fraccion(nombre: &str, valor: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&valor) {
        Ok(valor)
    } else {
        Err(format!(
            "métrica inválida: {nombre} = {valor}, debería estar en [0, 1]"
        ))
    }
}

/// Estima beneficio y daño de `accion` con estas métricas. Un dato inválido o
/// inconsistente es un `Err`: el modelo no adivina.
pub fn simular(accion: &Accion, metricas: &Metricas) -> Result<Simulacion, String> {
    let tasa_de_error = fraccion("tasa de error", metricas.tasa_de_error)?;
    let aciertos = fraccion("aciertos de caché", metricas.aciertos_de_cache)?;
    let utilizacion = metricas.utilizacion;
    if !(utilizacion.is_finite() && utilizacion >= 0.0) {
        return Err(format!(
            "métrica inválida: utilización = {utilizacion}, debería ser finita y no negativa"
        ));
    }

    match accion {
        Accion::Reiniciar { .. } => {
            if metricas.replicas == 0 {
                return Err(
                    "sin réplicas sanas, el modelo no estima el daño de reiniciar".to_string(),
                );
            }
            Ok(Simulacion {
                beneficio: tasa_de_error,
                dano: 1.0 / f64::from(metricas.replicas),
            })
        }
        Accion::EscalarReplicas {
            actuales, nuevas, ..
        } => {
            if *actuales != metricas.replicas {
                return Err(format!(
                    "la acción parte de {actuales} réplicas y las métricas dicen {}",
                    metricas.replicas
                ));
            }
            if *actuales == 0 || *nuevas == 0 {
                return Err("escalar desde o hacia cero réplicas no se modela".to_string());
            }
            let antes = rechazo(utilizacion);
            let despues = rechazo(utilizacion * f64::from(*actuales) / f64::from(*nuevas));
            Ok(Simulacion {
                beneficio: (antes - despues).max(0.0),
                dano: (despues - antes).max(0.0),
            })
        }
        Accion::VaciarCache { .. } => Ok(Simulacion {
            beneficio: tasa_de_error,
            dano: aciertos * utilizacion.min(1.0),
        }),
    }
}
