//! Juicio de salud con histéresis.
//!
//! Reemplazar un proceso tiene costo, así que un pico aislado no alcanza: un
//! slot se reemplaza solo después de `persistencia` mediciones críticas
//! **consecutivas**. Una medición sana reinicia la cuenta; una de aviso la deja
//! como está.
//!
//! Y un dato ausente no es un dato sano: si no se pudo medir nada, el
//! veredicto es [`Veredicto::SinDatos`] y la cuenta no se toca en ningún
//! sentido.

use std::collections::HashMap;
use std::time::Duration;

use crate::SlotId;

/// Umbrales de aviso y críticos por métrica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Umbrales {
    /// Memoria residente que merece atención.
    pub memoria_aviso: u64,
    /// Memoria residente que cuenta como crítica.
    pub memoria_critica: u64,
    /// Latencia que merece atención.
    pub latencia_aviso: Duration,
    /// Latencia que cuenta como crítica.
    pub latencia_critica: Duration,
    /// Mediciones críticas consecutivas antes de reemplazar.
    pub persistencia: u32,
}

impl Default for Umbrales {
    fn default() -> Self {
        Self {
            memoria_aviso: 256 << 20,
            memoria_critica: 512 << 20,
            latencia_aviso: Duration::from_millis(200),
            latencia_critica: Duration::from_secs(1),
            persistencia: 3,
        }
    }
}

/// Lo que se pudo medir de un proceso. Lo que no se midió queda en `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Medicion {
    /// Memoria residente, en bytes.
    pub memoria_bytes: Option<u64>,
    /// Latencia de respuesta.
    pub latencia: Option<Duration>,
}

/// Decisión sobre un slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredicto {
    /// No se pudo medir nada: la cuenta queda igual.
    SinDatos,
    /// Todo dentro de lo normal: la cuenta vuelve a cero.
    Sano,
    /// Alguna métrica en aviso: la cuenta queda igual.
    Aviso,
    /// Alguna métrica crítica, todavía sin alcanzar la persistencia.
    Critico {
        /// Mediciones críticas seguidas, contando esta.
        consecutivas: u32,
    },
    /// Crítico durante `persistencia` mediciones seguidas: hay que reemplazarlo.
    Reemplazar,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Nivel {
    Sano,
    Aviso,
    Critico,
}

fn nivel<T: PartialOrd>(valor: T, aviso: T, critico: T) -> Nivel {
    if valor >= critico {
        Nivel::Critico
    } else if valor >= aviso {
        Nivel::Aviso
    } else {
        Nivel::Sano
    }
}

/// Lleva la cuenta de mediciones críticas por slot.
#[derive(Debug, Clone, Default)]
pub struct Juez {
    umbrales: Umbrales,
    consecutivas: HashMap<SlotId, u32>,
}

impl Juez {
    /// Un juez con estos umbrales.
    pub fn nuevo(umbrales: Umbrales) -> Self {
        Self {
            umbrales,
            consecutivas: HashMap::new(),
        }
    }

    /// Evalúa una medición del slot. La peor métrica medida decide.
    pub fn evaluar(&mut self, slot: SlotId, m: &Medicion) -> Veredicto {
        let u = &self.umbrales;
        let niveles = [
            m.memoria_bytes
                .map(|v| nivel(v, u.memoria_aviso, u.memoria_critica)),
            m.latencia
                .map(|v| nivel(v, u.latencia_aviso, u.latencia_critica)),
        ];
        let Some(peor) = niveles.into_iter().flatten().max() else {
            return Veredicto::SinDatos;
        };

        let cuenta = self.consecutivas.entry(slot).or_insert(0);
        match peor {
            Nivel::Sano => {
                *cuenta = 0;
                Veredicto::Sano
            }
            Nivel::Aviso => Veredicto::Aviso,
            Nivel::Critico => {
                *cuenta += 1;
                if *cuenta >= u.persistencia {
                    Veredicto::Reemplazar
                } else {
                    Veredicto::Critico {
                        consecutivas: *cuenta,
                    }
                }
            }
        }
    }

    /// Olvida la historia de un slot (tras reemplazarlo, el proceso es otro).
    pub fn olvidar(&mut self, slot: SlotId) {
        self.consecutivas.remove(&slot);
    }
}

/// Memoria residente de un proceso, leída de `/proc/<pid>/status` (Linux).
/// En otros sistemas devuelve `None`: sin dato, no se inventa uno.
pub fn memoria_residente(pid: u32) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let estado = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let linea = estado.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kib: u64 = linea.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;

    fn juez() -> Juez {
        Juez::nuevo(Umbrales {
            memoria_aviso: 100 * MIB,
            memoria_critica: 200 * MIB,
            latencia_aviso: Duration::from_millis(100),
            latencia_critica: Duration::from_millis(500),
            persistencia: 3,
        })
    }

    fn mem(mib: u64) -> Medicion {
        Medicion {
            memoria_bytes: Some(mib * MIB),
            latencia: None,
        }
    }

    #[test]
    fn un_pico_aislado_no_reemplaza() {
        let mut j = juez();
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Critico { consecutivas: 1 }
        );
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Critico { consecutivas: 2 }
        );
        assert_eq!(j.evaluar(1, &mem(50)), Veredicto::Sano);
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Critico { consecutivas: 1 },
            "la cuenta volvió a cero"
        );
    }

    #[test]
    fn la_persistencia_critica_reemplaza() {
        let mut j = juez();
        j.evaluar(1, &mem(300));
        j.evaluar(1, &mem(300));
        assert_eq!(j.evaluar(1, &mem(300)), Veredicto::Reemplazar);
    }

    #[test]
    fn el_aviso_ni_suma_ni_reinicia() {
        let mut j = juez();
        j.evaluar(1, &mem(300));
        assert_eq!(j.evaluar(1, &mem(150)), Veredicto::Aviso);
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Critico { consecutivas: 2 }
        );
    }

    #[test]
    fn un_dato_ausente_no_es_un_dato_sano() {
        let mut j = juez();
        j.evaluar(1, &mem(300));
        j.evaluar(1, &mem(300));
        assert_eq!(j.evaluar(1, &Medicion::default()), Veredicto::SinDatos);
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Reemplazar,
            "la cuenta no se reinició"
        );
    }

    #[test]
    fn la_peor_metrica_decide() {
        let mut j = juez();
        let m = Medicion {
            memoria_bytes: Some(10 * MIB),
            latencia: Some(Duration::from_secs(2)),
        };
        assert_eq!(j.evaluar(1, &m), Veredicto::Critico { consecutivas: 1 });
    }

    #[test]
    fn cada_slot_lleva_su_propia_cuenta_y_olvidar_la_borra() {
        let mut j = juez();
        j.evaluar(1, &mem(300));
        j.evaluar(1, &mem(300));
        assert_eq!(
            j.evaluar(2, &mem(300)),
            Veredicto::Critico { consecutivas: 1 }
        );
        j.olvidar(1);
        assert_eq!(
            j.evaluar(1, &mem(300)),
            Veredicto::Critico { consecutivas: 1 }
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn la_memoria_residente_del_propio_proceso_es_positiva() {
        assert!(memoria_residente(std::process::id()).is_some_and(|b| b > 0));
    }
}
