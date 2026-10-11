//! El grupo de procesos trabajadores, supervisado por **posición**.
//!
//! Cada posición tiene un id estable; el proceso que la ocupa es desechable.
//!
//! - **Reciclar** lanza un proceso nuevo y recién entonces termina el viejo, en la misma
//!   posición: si el nuevo no arranca, el viejo sigue atendiendo.
//! - **Escalar** respeta la política: nunca menos que el mínimo ni más que el máximo.
//! - **El tick** reemplaza los procesos que murieron y recicla los que cumplieron su tiempo de
//!   vida, y dice qué hizo.
//! - **Ningún huérfano:** al soltar el grupo —también durante un pánico— se terminan y se
//!   esperan todos sus procesos.
//!
//! El grupo no sabe qué es un proceso: lo pide a una [`Fabrica`]. La real, [`FabricaDeProcesos`],
//! lanza un programa; las pruebas usan una de mentira, determinista, que no lanza nada.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use crate::reloj::Reloj;

/// Posiciones que puede tener un grupo, como máximo.
pub const TOPE_DE_POSICIONES: usize = 16;

/// Un proceso trabajador en marcha.
pub trait Proceso: Send {
    /// Su identificador en el sistema.
    fn pid(&self) -> u32;
    /// Si sigue corriendo.
    fn sigue_vivo(&mut self) -> bool;
    /// Lo termina y lo espera. Llamarlo dos veces no hace nada la segunda.
    fn terminar(&mut self);
}

/// Lo que lanza procesos para el grupo.
pub trait Fabrica: Send {
    /// Lanza un proceso nuevo.
    fn lanzar(&mut self) -> io::Result<Box<dyn Proceso>>;
}

/// Lanza un programa real. Su entrada estándar queda abierta y en manos del grupo: un
/// trabajador bien hecho termina cuando se cierra, así que tampoco queda huérfano si la
/// consola muere de golpe.
#[derive(Debug, Clone)]
pub struct FabricaDeProcesos {
    programa: PathBuf,
    argumentos: Vec<String>,
}

impl FabricaDeProcesos {
    /// Una fábrica que lanza `programa` con `argumentos`.
    pub fn nueva(programa: impl Into<PathBuf>, argumentos: Vec<String>) -> FabricaDeProcesos {
        FabricaDeProcesos {
            programa: programa.into(),
            argumentos,
        }
    }
}

impl Fabrica for FabricaDeProcesos {
    fn lanzar(&mut self) -> io::Result<Box<dyn Proceso>> {
        let hijo = Command::new(&self.programa)
            .args(&self.argumentos)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(ProcesoHijo {
            hijo,
            terminado: false,
        }))
    }
}

/// Un proceso hijo real. Si se lo suelta sin terminarlo, se termina igual.
#[derive(Debug)]
struct ProcesoHijo {
    hijo: Child,
    terminado: bool,
}

impl Proceso for ProcesoHijo {
    fn pid(&self) -> u32 {
        self.hijo.id()
    }

    fn sigue_vivo(&mut self) -> bool {
        !self.terminado && matches!(self.hijo.try_wait(), Ok(None))
    }

    fn terminar(&mut self) {
        if !self.terminado {
            let _ = self.hijo.kill();
            let _ = self.hijo.wait();
            self.terminado = true;
        }
    }
}

impl Drop for ProcesoHijo {
    fn drop(&mut self) {
        self.terminar();
    }
}

/// Cuántas posiciones puede tener el grupo y cuánto vive cada proceso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Politica {
    min: usize,
    max: usize,
    vida_maxima: Duration,
}

impl Politica {
    /// Una política, si es válida: `1 ≤ min ≤ max ≤` [`TOPE_DE_POSICIONES`] y al menos un
    /// segundo de vida.
    pub fn nueva(min: usize, max: usize, vida_maxima: Duration) -> Result<Politica, ErrorGrupo> {
        if min < 1 || min > max || max > TOPE_DE_POSICIONES {
            return Err(ErrorGrupo::PoliticaInvalida(format!(
                "se necesita 1 ≤ mínimo ≤ máximo ≤ {TOPE_DE_POSICIONES}; llegó mínimo {min}, \
                 máximo {max}"
            )));
        }
        if vida_maxima < Duration::from_secs(1) {
            return Err(ErrorGrupo::PoliticaInvalida(
                "el tiempo de vida tiene que ser de al menos un segundo".into(),
            ));
        }
        Ok(Politica {
            min,
            max,
            vida_maxima,
        })
    }

    /// El mínimo de posiciones.
    pub fn min(&self) -> usize {
        self.min
    }

    /// El máximo de posiciones.
    pub fn max(&self) -> usize {
        self.max
    }

    /// Cuánto vive un proceso antes de que el tick lo recicle.
    pub fn vida_maxima(&self) -> Duration {
        self.vida_maxima
    }
}

/// El estado de una posición.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Corriendo, con el proceso con el que nació la posición.
    Viva,
    /// Corriendo, con un proceso que reemplazó a otro.
    Reciclada,
    /// Su proceso terminó; el próximo tick lo reemplaza.
    Muerta,
}

impl Estado {
    /// El nombre que usa la API.
    pub fn nombre(&self) -> &'static str {
        match self {
            Estado::Viva => "viva",
            Estado::Reciclada => "reciclada",
            Estado::Muerta => "muerta",
        }
    }
}

/// Por qué se reemplazó un proceso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    /// Lo pidió una persona.
    Manual,
    /// El proceso había muerto.
    Murio,
    /// Cumplió su tiempo de vida.
    VidaCumplida,
}

impl Motivo {
    /// El nombre que usa la API.
    pub fn nombre(&self) -> &'static str {
        match self {
            Motivo::Manual => "manual",
            Motivo::Murio => "murio",
            Motivo::VidaCumplida => "vida_cumplida",
        }
    }
}

/// Un reemplazo: en qué posición, por qué, y cómo salió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accion {
    /// La posición.
    pub posicion: u32,
    /// Por qué.
    pub motivo: Motivo,
    /// El proceso que se reemplazó.
    pub pid_anterior: u32,
    /// El proceso nuevo, o por qué no se pudo lanzar (entonces el anterior sigue en su lugar).
    pub resultado: Result<u32, String>,
}

/// Lo que se ve de una posición.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VistaPosicion {
    /// El id de la posición: estable a través de los reciclajes.
    pub id: u32,
    /// El proceso que la ocupa.
    pub pid: u32,
    /// Su estado.
    pub estado: Estado,
    /// Milisegundos desde el último cambio de estado.
    pub hace_ms: u64,
    /// Milisegundos de vida del proceso actual.
    pub edad_ms: u64,
    /// Cuántos procesos ocuparon la posición antes que el actual.
    pub generacion: u32,
}

/// Lo que se ve del grupo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VistaGrupo {
    /// La política vigente.
    pub politica: Politica,
    /// Las posiciones, en orden.
    pub posiciones: Vec<VistaPosicion>,
}

/// El resultado de escalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escalado {
    /// Posiciones antes.
    pub antes: usize,
    /// Posiciones después.
    pub despues: usize,
}

/// Lo que puede fallar al operar el grupo.
#[derive(Debug)]
pub enum ErrorGrupo {
    /// La política pedida no es válida.
    PoliticaInvalida(String),
    /// La cantidad pedida no respeta la política.
    FueraDePolitica {
        /// Lo pedido.
        cantidad: usize,
        /// El mínimo vigente.
        min: usize,
        /// El máximo vigente.
        max: usize,
    },
    /// No hay ninguna posición con ese id.
    PosicionDesconocida(u32),
    /// Un proceso no se pudo lanzar.
    Lanzar(io::Error),
}

impl fmt::Display for ErrorGrupo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorGrupo::PoliticaInvalida(motivo) => f.write_str(motivo),
            ErrorGrupo::FueraDePolitica { cantidad, min, max } => write!(
                f,
                "no se puede quedar en {cantidad}: la política pide entre {min} y {max} \
                 posiciones"
            ),
            ErrorGrupo::PosicionDesconocida(id) => write!(f, "no hay ninguna posición {id}"),
            ErrorGrupo::Lanzar(e) => write!(f, "no se pudo lanzar el proceso: {e}"),
        }
    }
}

impl std::error::Error for ErrorGrupo {}

struct Posicion {
    id: u32,
    proceso: Box<dyn Proceso>,
    estado: Estado,
    desde_ms: u64,
    lanzado_ms: u64,
    generacion: u32,
}

/// El grupo de trabajadores.
pub struct Grupo {
    posiciones: Vec<Posicion>,
    politica: Politica,
    fabrica: Box<dyn Fabrica>,
    reloj: Arc<dyn Reloj>,
    siguiente_id: u32,
}

impl fmt::Debug for Grupo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grupo")
            .field("posiciones", &self.posiciones.len())
            .field("politica", &self.politica)
            .finish()
    }
}

impl Grupo {
    /// Un grupo con el mínimo de la política, ya lanzado. Si algún proceso no arranca, los ya
    /// lanzados se terminan y se devuelve el error.
    pub fn nuevo(
        politica: Politica,
        fabrica: Box<dyn Fabrica>,
        reloj: Arc<dyn Reloj>,
    ) -> Result<Grupo, ErrorGrupo> {
        let mut grupo = Grupo {
            posiciones: Vec::new(),
            politica,
            fabrica,
            reloj,
            siguiente_id: 1,
        };
        for _ in 0..politica.min {
            grupo.agregar()?;
        }
        Ok(grupo)
    }

    fn agregar(&mut self) -> Result<u32, ErrorGrupo> {
        let proceso = self.fabrica.lanzar().map_err(ErrorGrupo::Lanzar)?;
        let ahora = self.reloj.ahora_ms();
        let id = self.siguiente_id;
        self.siguiente_id += 1;
        self.posiciones.push(Posicion {
            id,
            proceso,
            estado: Estado::Viva,
            desde_ms: ahora,
            lanzado_ms: ahora,
            generacion: 0,
        });
        Ok(id)
    }

    /// La política vigente.
    pub fn politica(&self) -> Politica {
        self.politica
    }

    /// Cuántas posiciones hay.
    pub fn cantidad(&self) -> usize {
        self.posiciones.len()
    }

    /// Marca como muertas las posiciones cuyo proceso terminó.
    fn revisar(&mut self) {
        let ahora = self.reloj.ahora_ms();
        for p in &mut self.posiciones {
            if p.estado != Estado::Muerta && !p.proceso.sigue_vivo() {
                p.estado = Estado::Muerta;
                p.desde_ms = ahora;
            }
        }
    }

    /// Lo que se ve del grupo ahora (antes, revisa quién sigue vivo).
    pub fn vista(&mut self) -> VistaGrupo {
        self.revisar();
        let ahora = self.reloj.ahora_ms();
        VistaGrupo {
            politica: self.politica,
            posiciones: self
                .posiciones
                .iter()
                .map(|p| VistaPosicion {
                    id: p.id,
                    pid: p.proceso.pid(),
                    estado: p.estado,
                    hace_ms: ahora.saturating_sub(p.desde_ms),
                    edad_ms: ahora.saturating_sub(p.lanzado_ms),
                    generacion: p.generacion,
                })
                .collect(),
        }
    }

    /// Reemplaza el proceso de la posición `id`, que **conserva su lugar y su id**.
    pub fn reciclar(&mut self, id: u32) -> Result<Accion, ErrorGrupo> {
        let indice = self
            .posiciones
            .iter()
            .position(|p| p.id == id)
            .ok_or(ErrorGrupo::PosicionDesconocida(id))?;
        let accion = self.reemplazar(indice, Motivo::Manual);
        match accion.resultado {
            Ok(_) => Ok(accion),
            Err(motivo) => Err(ErrorGrupo::Lanzar(io::Error::other(motivo))),
        }
    }

    /// Lanza el reemplazo y, solo si arrancó, termina al anterior.
    fn reemplazar(&mut self, indice: usize, motivo: Motivo) -> Accion {
        let lanzado = self.fabrica.lanzar();
        let ahora = self.reloj.ahora_ms();
        let posicion = &mut self.posiciones[indice];
        let pid_anterior = posicion.proceso.pid();
        let resultado = match lanzado {
            Ok(nuevo) => {
                let pid_nuevo = nuevo.pid();
                let mut viejo = std::mem::replace(&mut posicion.proceso, nuevo);
                viejo.terminar();
                posicion.estado = Estado::Reciclada;
                posicion.desde_ms = ahora;
                posicion.lanzado_ms = ahora;
                posicion.generacion += 1;
                Ok(pid_nuevo)
            }
            Err(e) => Err(e.to_string()),
        };
        Accion {
            posicion: posicion.id,
            motivo,
            pid_anterior,
            resultado,
        }
    }

    /// Lleva el grupo a `cantidad` posiciones, si la política lo permite. Al bajar, salen las
    /// posiciones más nuevas. Si al subir un proceso no arranca, se devuelve el error y quedan
    /// las posiciones que sí se lanzaron.
    pub fn escalar(&mut self, cantidad: usize) -> Result<Escalado, ErrorGrupo> {
        let (min, max) = (self.politica.min, self.politica.max);
        if cantidad < min || cantidad > max {
            return Err(ErrorGrupo::FueraDePolitica { cantidad, min, max });
        }
        let antes = self.cantidad();
        while self.cantidad() < cantidad {
            self.agregar()?;
        }
        while self.cantidad() > cantidad {
            if let Some(mut p) = self.posiciones.pop() {
                p.proceso.terminar();
            }
        }
        Ok(Escalado {
            antes,
            despues: self.cantidad(),
        })
    }

    /// Cambia el mínimo y el máximo, y ajusta el grupo para que los respete.
    pub fn cambiar_politica(&mut self, min: usize, max: usize) -> Result<Escalado, ErrorGrupo> {
        let nueva = Politica::nueva(min, max, self.politica.vida_maxima)?;
        self.politica = nueva;
        let objetivo = self.cantidad().clamp(min, max);
        self.escalar(objetivo)
    }

    /// Reemplaza los procesos que murieron y recicla los que cumplieron su tiempo de vida.
    /// Devuelve lo que hizo.
    pub fn tick(&mut self) -> Vec<Accion> {
        self.revisar();
        let ahora = self.reloj.ahora_ms();
        let vida = u64::try_from(self.politica.vida_maxima.as_millis()).unwrap_or(u64::MAX);
        let mut acciones = Vec::new();
        for indice in 0..self.posiciones.len() {
            let posicion = &self.posiciones[indice];
            let motivo = if posicion.estado == Estado::Muerta {
                Motivo::Murio
            } else if ahora.saturating_sub(posicion.lanzado_ms) >= vida {
                Motivo::VidaCumplida
            } else {
                continue;
            };
            acciones.push(self.reemplazar(indice, motivo));
        }
        acciones
    }
}

impl Drop for Grupo {
    fn drop(&mut self) {
        for p in &mut self.posiciones {
            p.proceso.terminar();
        }
    }
}
