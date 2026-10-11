//! El reloj, inyectable: el real para producción y uno de mentira para que las pruebas de
//! plazos y cachés no tengan que dormir.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Una fuente de tiempo monótono, en milisegundos desde un origen arbitrario.
pub trait Reloj: Send + Sync {
    /// Milisegundos transcurridos.
    fn ahora_ms(&self) -> u64;
    /// Espera `pausa`.
    fn dormir(&self, pausa: Duration);
}

/// El reloj del sistema.
#[derive(Debug)]
pub struct RelojReal {
    origen: Instant,
}

impl RelojReal {
    /// Un reloj que empieza en cero ahora.
    pub fn nuevo() -> RelojReal {
        RelojReal {
            origen: Instant::now(),
        }
    }
}

impl Default for RelojReal {
    fn default() -> RelojReal {
        RelojReal::nuevo()
    }
}

impl Reloj for RelojReal {
    fn ahora_ms(&self) -> u64 {
        u64::try_from(self.origen.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn dormir(&self, pausa: Duration) {
        thread::sleep(pausa);
    }
}

/// Un reloj que solo avanza cuando se lo piden: `dormir` adelanta el tiempo al instante, sin
/// esperar de verdad.
#[derive(Debug, Default)]
pub struct RelojDeMentira {
    ms: AtomicU64,
}

impl RelojDeMentira {
    /// Un reloj detenido en cero.
    pub fn nuevo() -> RelojDeMentira {
        RelojDeMentira::default()
    }

    /// Adelanta el reloj.
    pub fn avanzar(&self, cuanto: Duration) {
        let ms = u64::try_from(cuanto.as_millis()).unwrap_or(u64::MAX);
        self.ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Reloj for RelojDeMentira {
    fn ahora_ms(&self) -> u64 {
        self.ms.load(Ordering::SeqCst)
    }

    fn dormir(&self, pausa: Duration) {
        self.avanzar(pausa);
    }
}
