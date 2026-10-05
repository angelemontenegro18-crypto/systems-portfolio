//! Piezas comunes de los tests: una política de ejemplo, propuestas que pasan
//! las cuatro puertas y un actuador que registra el orden de las llamadas.

#![allow(dead_code)] // cada archivo de tests usa una parte

pub mod llegadas;

use autonomous_remediation::{
    Accion, Actuador, Envolvente, Metricas, Permiso, Politica, Propuesta, Servicio, Umbrales,
};

/// Una llamada al actuador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Llamada {
    Armar,
    Verificar,
    Aplicar,
    Comprobar,
    Revertir,
}

/// Actuador de prueba: registra cada llamada, en orden, y falla en las que se
/// le indiquen.
#[derive(Debug, Default)]
pub struct ActuadorDePrueba {
    pub llamadas: Vec<Llamada>,
    pub falla_en: Vec<Llamada>,
}

impl ActuadorDePrueba {
    pub fn que_falla_en(llamadas: &[Llamada]) -> ActuadorDePrueba {
        ActuadorDePrueba {
            llamadas: Vec::new(),
            falla_en: llamadas.to_vec(),
        }
    }

    fn registrar(&mut self, llamada: Llamada) -> Result<(), String> {
        self.llamadas.push(llamada);
        if self.falla_en.contains(&llamada) {
            Err(format!("falla simulada en {llamada:?}"))
        } else {
            Ok(())
        }
    }
}

impl Actuador for ActuadorDePrueba {
    fn armar_reversion(&mut self, _: &Accion) -> Result<(), String> {
        self.registrar(Llamada::Armar)
    }
    fn verificar_reversion(&mut self, _: &Accion) -> Result<(), String> {
        self.registrar(Llamada::Verificar)
    }
    fn aplicar(&mut self, _: &Accion) -> Result<(), String> {
        self.registrar(Llamada::Aplicar)
    }
    fn comprobar(&mut self, _: &Accion) -> Result<(), String> {
        self.registrar(Llamada::Comprobar)
    }
    fn revertir(&mut self, _: &Accion) -> Result<(), String> {
        self.registrar(Llamada::Revertir)
    }
}

pub fn servicio(nombre: &str) -> Servicio {
    Servicio::nuevo(nombre).expect("nombre válido")
}

/// `api` puede reiniciarse, vaciar su caché y escalar entre 2 y 6 réplicas;
/// `catalogo` solo puede vaciar su caché. Confianza mínima 0.90, daño máximo 0.30.
pub fn envolvente() -> Envolvente {
    Envolvente::vacia()
        .con(Permiso::reiniciar(servicio("api")))
        .con(Permiso::vaciar_cache(servicio("api")))
        .con(Permiso::escalar(servicio("api"), 2, 6).expect("rango válido"))
        .con(Permiso::vaciar_cache(servicio("catalogo")))
}

pub fn umbrales() -> Umbrales {
    Umbrales::nuevos(0.90, 0.30).expect("umbrales válidos")
}

pub fn politica() -> Politica {
    Politica::nueva(envolvente(), umbrales())
}

pub fn metricas(
    tasa_de_error: f64,
    utilizacion: f64,
    aciertos_de_cache: f64,
    replicas: u32,
) -> Metricas {
    Metricas {
        tasa_de_error,
        utilizacion,
        aciertos_de_cache,
        replicas,
    }
}

pub fn propuesta(accion: Accion, confianza: Option<f64>, metricas: Option<Metricas>) -> Propuesta {
    Propuesta {
        accion,
        motivo: "señal de prueba".to_string(),
        confianza,
        metricas,
    }
}

/// Reiniciar `api`: beneficio 0.30, daño 0.25 (una de cuatro réplicas).
pub fn reiniciar_sano() -> Propuesta {
    propuesta(
        Accion::Reiniciar {
            servicio: servicio("api"),
        },
        Some(0.97),
        Some(metricas(0.30, 0.6, 0.5, 4)),
    )
}

/// Escalar `api` de 3 a 5: la utilización baja de 1.4 a 0.84; beneficio 0.29.
pub fn escalar_sano() -> Propuesta {
    propuesta(
        Accion::EscalarReplicas {
            servicio: servicio("api"),
            actuales: 3,
            nuevas: 5,
        },
        Some(0.97),
        Some(metricas(0.0, 1.4, 0.5, 3)),
    )
}

/// Vaciar la caché de `api`: beneficio 0.20, daño 0.2 × 0.5 = 0.10.
pub fn vaciar_sano() -> Propuesta {
    propuesta(
        Accion::VaciarCache {
            servicio: servicio("api"),
        },
        Some(0.97),
        Some(metricas(0.20, 0.5, 0.2, 4)),
    )
}
