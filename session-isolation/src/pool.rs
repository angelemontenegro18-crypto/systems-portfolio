//! Pool de procesos residentes con reemplazo en caliente.
//!
//! Cada **slot** tiene una identidad estable (id, etiqueta, receta, fecha de
//! creación) y un proceso que la encarna. El proceso es desechable; el slot no.
//! Reemplazar un slot cambia el proceso y conserva todo lo demás.
//!
//! ## El orden del reemplazo
//!
//! El proceso nuevo se lanza **antes** de terminar el viejo. Así la capacidad
//! del pool nunca cae durante un reemplazo, y si el nuevo no arranca, el viejo
//! sigue atendiendo: un reemplazo fallido no puede dejar un slot vacío.
//!
//! Eso exige un proceso de más durante el cambio. El pool lo reserva como
//! **margen de reemplazo** — la misma idea que `maxSurge` en las
//! actualizaciones continuas de Kubernetes. Sin margen, con el pool lleno, el
//! reemplazo se rechaza en vez de arriesgar el slot.

use std::fmt;
use std::io;
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use crate::ipc::terminar;
use crate::salud::{Juez, Medicion, Veredicto};
use crate::{Receta, SlotId};

/// Identidad de un slot: lo que sobrevive a los reemplazos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaSlot {
    /// Identificador estable.
    pub id: SlotId,
    /// Nombre para mostrar.
    pub etiqueta: String,
    /// Cómo se lanza su proceso.
    pub receta: Receta,
    /// Cuándo se creó el slot (no el proceso).
    pub creado: Instant,
    /// Cuántas veces se reemplazó su proceso.
    pub reemplazos: u32,
}

/// Foto de un slot para mostrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VistaSlot {
    /// La identidad del slot.
    pub meta: MetaSlot,
    /// Pid del proceso actual.
    pub pid: u32,
    /// Cuándo arrancó el proceso actual.
    pub proceso_desde: Instant,
}

/// Lo que puede fallar al operar el pool.
#[derive(Debug)]
pub enum ErrorPool {
    /// No entra otro slot.
    SinCupo {
        /// Máximo de slots.
        max: usize,
    },
    /// No hay margen para tener dos procesos a la vez durante el reemplazo.
    SinMargen {
        /// Procesos que habría durante el cambio.
        procesos: usize,
        /// Máximo permitido (slots + margen).
        tope: usize,
    },
    /// No existe ese slot.
    NoExiste(SlotId),
    /// El proceso no se pudo lanzar. Si era un reemplazo, el anterior sigue vivo.
    Lanzar(io::Error),
}

impl fmt::Display for ErrorPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SinCupo { max } => write!(f, "el pool ya tiene {max} slots"),
            Self::SinMargen { procesos, tope } => {
                write!(
                    f,
                    "el reemplazo necesita {procesos} procesos a la vez y el tope es {tope}"
                )
            }
            Self::NoExiste(id) => write!(f, "no existe el slot {id}"),
            Self::Lanzar(e) => write!(f, "no se pudo lanzar el proceso: {e}"),
        }
    }
}

impl std::error::Error for ErrorPool {}

/// Qué pasó en un ciclo de salud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evento {
    /// Se reemplazó el proceso de un slot.
    Reemplazado {
        /// El slot.
        slot: SlotId,
        /// Pid del proceso que se terminó.
        pid_anterior: u32,
        /// Pid del proceso nuevo.
        pid_nuevo: u32,
        /// Por qué.
        motivo: String,
    },
    /// Hacía falta reemplazar y no se pudo. El proceso anterior sigue vivo.
    ReemplazoFallido {
        /// El slot.
        slot: SlotId,
        /// Por qué hacía falta.
        motivo: String,
        /// Por qué no se pudo.
        error: String,
    },
}

struct Slot {
    meta: MetaSlot,
    proceso: Child,
    desde: Instant,
}

/// Pool de procesos residentes.
pub struct Pool {
    max_slots: usize,
    margen: usize,
    slots: Vec<Slot>,
    siguiente_id: SlotId,
}

fn lanzar_proceso(receta: &Receta) -> io::Result<Child> {
    // stdin queda abierto y en manos del pool: el proceso vive mientras el pool
    // lo tenga, y termina ordenadamente cuando el pool lo suelta.
    Command::new(&receta.programa)
        .args(&receta.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

impl Pool {
    /// Un pool de hasta `max_slots` slots, con `margen_de_reemplazo` procesos
    /// extra permitidos durante un reemplazo.
    pub fn nuevo(max_slots: usize, margen_de_reemplazo: usize) -> Self {
        Self {
            max_slots,
            margen: margen_de_reemplazo,
            slots: Vec::new(),
            siguiente_id: 1,
        }
    }

    /// Crea un slot y lanza su proceso.
    pub fn lanzar(
        &mut self,
        etiqueta: impl Into<String>,
        receta: Receta,
    ) -> Result<SlotId, ErrorPool> {
        if self.slots.len() >= self.max_slots {
            return Err(ErrorPool::SinCupo {
                max: self.max_slots,
            });
        }
        let proceso = lanzar_proceso(&receta).map_err(ErrorPool::Lanzar)?;
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        let ahora = Instant::now();
        self.slots.push(Slot {
            meta: MetaSlot {
                id,
                etiqueta: etiqueta.into(),
                receta,
                creado: ahora,
                reemplazos: 0,
            },
            proceso,
            desde: ahora,
        });
        Ok(id)
    }

    /// Reemplaza el proceso de un slot con la misma receta.
    pub fn reemplazar(&mut self, id: SlotId, motivo: &str) -> Result<Evento, ErrorPool> {
        self.reemplazar_con_receta(id, None, motivo)
    }

    /// Reemplaza el proceso de un slot, opcionalmente con una receta nueva
    /// (una actualización continua, slot por slot). Si el proceso nuevo no
    /// arranca, el slot conserva **su receta y su proceso anteriores**.
    pub fn reemplazar_con_receta(
        &mut self,
        id: SlotId,
        nueva: Option<Receta>,
        motivo: &str,
    ) -> Result<Evento, ErrorPool> {
        let procesos = self.slots.len() + 1;
        let tope = self.max_slots + self.margen;
        let indice = self.indice(id)?;
        if procesos > tope {
            return Err(ErrorPool::SinMargen { procesos, tope });
        }

        let receta = nueva.unwrap_or_else(|| self.slots[indice].meta.receta.clone());
        // 1. El nuevo arranca primero. Si falla, se sale acá y nada cambió.
        let proceso_nuevo = lanzar_proceso(&receta).map_err(ErrorPool::Lanzar)?;
        let pid_nuevo = proceso_nuevo.id();

        // 2. Recién ahora se retira el viejo.
        let slot = &mut self.slots[indice];
        let mut viejo = std::mem::replace(&mut slot.proceso, proceso_nuevo);
        let pid_anterior = viejo.id();
        terminar(&mut viejo);

        slot.desde = Instant::now();
        slot.meta.receta = receta;
        slot.meta.reemplazos += 1;
        Ok(Evento::Reemplazado {
            slot: id,
            pid_anterior,
            pid_nuevo,
            motivo: motivo.to_string(),
        })
    }

    /// Termina el proceso de un slot y lo quita del pool.
    pub fn retirar(&mut self, id: SlotId) -> Result<(), ErrorPool> {
        let indice = self.indice(id)?;
        let mut slot = self.slots.remove(indice);
        terminar(&mut slot.proceso);
        Ok(())
    }

    /// Slots cuyo proceso terminó por su cuenta, con su código de salida.
    pub fn caidos(&mut self) -> Vec<(SlotId, Option<i32>)> {
        self.slots
            .iter_mut()
            .filter_map(|s| match s.proceso.try_wait() {
                Ok(Some(estado)) => Some((s.meta.id, estado.code())),
                _ => None,
            })
            .collect()
    }

    /// Un ciclo de salud: reemplaza los procesos caídos y los que el juez
    /// condena. `medir` recibe un pid y devuelve lo que se pudo medir de él.
    pub fn ciclo_de_salud(
        &mut self,
        juez: &mut Juez,
        mut medir: impl FnMut(u32) -> Medicion,
    ) -> Vec<Evento> {
        let mut eventos = Vec::new();

        for (id, codigo) in self.caidos() {
            let motivo = format!("el proceso terminó por su cuenta (código {codigo:?})");
            eventos.push(self.reemplazar_registrando(id, &motivo, juez));
        }

        let vivos: Vec<(SlotId, u32)> = self
            .slots
            .iter()
            .map(|s| (s.meta.id, s.proceso.id()))
            .collect();
        for (id, pid) in vivos {
            if juez.evaluar(id, &medir(pid)) == Veredicto::Reemplazar {
                let motivo = "métricas críticas sostenidas".to_string();
                eventos.push(self.reemplazar_registrando(id, &motivo, juez));
            }
        }
        eventos
    }

    fn reemplazar_registrando(&mut self, id: SlotId, motivo: &str, juez: &mut Juez) -> Evento {
        match self.reemplazar(id, motivo) {
            Ok(evento) => {
                juez.olvidar(id);
                evento
            }
            Err(e) => Evento::ReemplazoFallido {
                slot: id,
                motivo: motivo.to_string(),
                error: e.to_string(),
            },
        }
    }

    /// Foto del pool, en orden de creación.
    pub fn vista(&self) -> Vec<VistaSlot> {
        self.slots
            .iter()
            .map(|s| VistaSlot {
                meta: s.meta.clone(),
                pid: s.proceso.id(),
                proceso_desde: s.desde,
            })
            .collect()
    }

    /// Cuántos slots hay.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// `true` si no hay slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn indice(&self, id: SlotId) -> Result<usize, ErrorPool> {
        self.slots
            .iter()
            .position(|s| s.meta.id == id)
            .ok_or(ErrorPool::NoExiste(id))
    }
}

/// Al soltar el pool, todos sus procesos se terminan y se esperan.
impl Drop for Pool {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            terminar(&mut slot.proceso);
        }
    }
}
