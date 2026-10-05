//! Detector pasivo de cambios en el ritmo de llegadas.
//!
//! **Pasivo por tipo:** recibe los tiempos entre llegadas ya observados, como
//! `&[u64]`, y devuelve un valor. No hace E/S, no mide nada por su cuenta y no
//! tiene con qué emitir tráfico. El test `detector_pasivo` lo comprueba sobre
//! el código fuente.
//!
//! Es el CUSUM de Page (*Continuous inspection schemes*, Biometrika, 1954):
//! acumula el logaritmo del cociente de verosimilitud entre «el ritmo sigue
//! igual» y «cambió», con un piso en cero, y da la alarma cuando la suma pasa
//! un límite. Supone llegadas de Poisson —tiempos entre llegadas
//! exponenciales— y vigila dos cambios a la vez: que el ritmo baje a la mitad
//! y que se duplique.
//!
//! Para una exponencial de ritmo `λ`, el logaritmo del cociente entre `λ₁` y
//! `λ₀` en un tiempo `x` es `ln(λ₁/λ₀) − (λ₁ − λ₀)·x`. Con `r = x·λ₀`:
//!
//! - ritmo a la mitad: `−ln 2 + r/2`;
//! - ritmo al doble: `ln 2 − r`.
//!
//! El único logaritmo es la constante `ln 2`: no hay funciones trascendentes
//! en tiempo de ejecución, así que el resultado es el mismo, bit a bit, en
//! cualquier plataforma.

use std::f64::consts::LN_2;

/// Hacia dónde cambió el ritmo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sentido {
    /// Las llegadas se espaciaron: el ritmo bajó.
    MasLento,
    /// Las llegadas se juntaron: el ritmo subió.
    MasRapido,
}

/// La primera vez que la suma pasó el límite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alarma {
    /// Índice del intervalo en el que sonó.
    pub indice: usize,
    /// Hacia dónde cambió el ritmo.
    pub sentido: Sentido,
    /// La suma acumulada en ese momento.
    pub estadistico: f64,
}

/// Resultado de analizar una serie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analisis {
    /// La primera alarma, si hubo.
    pub alarma: Option<Alarma>,
    /// Desde la alarma hasta el final, la fracción de intervalos con la suma
    /// por encima del límite: cerca de 1 si el cambio se sostiene, baja si fue
    /// pasajero. Cero sin alarma.
    pub persistencia: f64,
}

/// El detector: el ritmo de referencia y el límite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cusum {
    media_de_referencia: f64,
    limite: f64,
}

impl Cusum {
    /// Con el tiempo medio entre llegadas de referencia y el límite de la
    /// suma. Los dos tienen que ser finitos y positivos.
    ///
    /// Un límite más alto da menos falsas alarmas y detecta más tarde: el
    /// límite es la escala logarítmica de cuánta evidencia se exige.
    pub fn nuevo(media_de_referencia: f64, limite: f64) -> Option<Cusum> {
        let valido = media_de_referencia.is_finite()
            && media_de_referencia > 0.0
            && limite.is_finite()
            && limite > 0.0;
        valido.then_some(Cusum {
            media_de_referencia,
            limite,
        })
    }

    /// Con la media de `muestra` como referencia: un tramo en el que se sabe
    /// que el servicio andaba bien. `None` si está vacía o su media es cero.
    pub fn con_referencia(muestra: &[u64], limite: f64) -> Option<Cusum> {
        if muestra.is_empty() {
            return None;
        }
        // Suma en orden fijo: el resultado no depende de la plataforma.
        let mut suma = 0.0;
        for &x in muestra {
            suma += x as f64;
        }
        Cusum::nuevo(suma / muestra.len() as f64, limite)
    }

    /// El tiempo medio de referencia.
    pub fn media_de_referencia(&self) -> f64 {
        self.media_de_referencia
    }

    /// Analiza los intervalos, en orden.
    pub fn analizar(&self, intervalos: &[u64]) -> Analisis {
        let (mut mas_lento, mut mas_rapido) = (0.0f64, 0.0f64);
        let mut alarma = None;
        let mut por_encima = 0usize;
        for (indice, &x) in intervalos.iter().enumerate() {
            let r = x as f64 / self.media_de_referencia;
            mas_lento = (mas_lento - LN_2 + r / 2.0).max(0.0);
            mas_rapido = (mas_rapido + LN_2 - r).max(0.0);
            let (estadistico, sentido) = if mas_lento >= mas_rapido {
                (mas_lento, Sentido::MasLento)
            } else {
                (mas_rapido, Sentido::MasRapido)
            };
            if estadistico > self.limite {
                if alarma.is_none() {
                    alarma = Some(Alarma {
                        indice,
                        sentido,
                        estadistico,
                    });
                }
                por_encima += 1;
            }
        }
        let persistencia = match alarma {
            Some(a) => por_encima as f64 / (intervalos.len() - a.indice) as f64,
            None => 0.0,
        };
        Analisis {
            alarma,
            persistencia,
        }
    }
}
