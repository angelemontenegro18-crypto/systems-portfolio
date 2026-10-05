//! # staged-agent
//!
//! Un agente que modifica archivos de un repositorio `git` en tres niveles de
//! confianza:
//!
//! 1. **Simular** ([`Agente::simular`]): calcula el [`Plan`] sobre lo
//!    commiteado en la rama destino. No escribe nada, ni en el árbol de
//!    trabajo ni en `.git`.
//! 2. **Preparar** ([`Agente::preparar`]): aplica el plan en un worktree
//!    aislado, sobre una rama nueva. El árbol principal y la rama destino
//!    quedan intactos. Si algo falla a mitad de camino, no queda ni el worktree
//!    ni la rama.
//! 3. **Aplicar** ([`Agente::aplicar`]): lleva a la rama destino **solo el plan
//!    exacto que se revisó**. Exige la confirmación explícita de una persona,
//!    un árbol limpio y que la huella recalculada desde la rama preparada
//!    coincida con la revisada.
//!
//! El trabajo concreto lo hacen las [`Regla`]s: aquí, normalizar archivos de
//! texto (finales de línea, espacios finales, salto final). Las rutas del plan
//! no pueden salir de la raíz ([`rutas`]).
//!
//! `git` es el del sistema, invocado sin shell, con argumentos fijos y siempre
//! con `-C <raíz>`.
//!
//! ```no_run
//! use staged_agent::{Agente, Confirmacion};
//!
//! # fn main() -> Result<(), staged_agent::Error> {
//! let agente = Agente::nuevo("ruta/al/repositorio");
//! let plan = agente.simular("main")?; // no escribe nada
//! println!("{plan}");
//!
//! let preparado = agente.preparar(&plan)?; // rama staged-agent/<huella>
//! // … revisar `git diff main staged-agent/<huella>` …
//!
//! let confirmacion = Confirmacion::escrita(plan.confirmacion_esperada());
//! agente.aplicar(&preparado, &plan.huella(), &confirmacion)?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod aplicacion;
mod error;
mod git;
mod lectura;
pub mod plan;
mod preparacion;
pub mod regla;
pub mod rutas;
mod simulacion;

use std::path::PathBuf;

pub use aplicacion::{Aplicado, Confirmacion};
pub use error::Error;
pub use git::{ErrorGit, Git};
pub use plan::{Cambio, Huella, Omision, Omitido, Plan};
pub use preparacion::{Preparado, PREFIJO_DE_RAMA};
pub use regla::{reglas_por_defecto, EspaciosFinales, FinesDeLinea, Regla, SaltoFinal};

/// El agente: un repositorio, un `git` y unas reglas.
pub struct Agente {
    raiz: PathBuf,
    git: Git,
    reglas: Vec<Box<dyn Regla>>,
}

impl Agente {
    /// Agente sobre el repositorio en `raiz` (o cualquier directorio dentro de
    /// él), con el `git` del sistema y las [`reglas_por_defecto`].
    pub fn nuevo(raiz: impl Into<PathBuf>) -> Agente {
        Agente {
            raiz: raiz.into(),
            git: Git::del_sistema(),
            reglas: reglas_por_defecto(),
        }
    }

    /// Reemplaza las reglas.
    pub fn con_reglas(mut self, reglas: Vec<Box<dyn Regla>>) -> Agente {
        self.reglas = reglas;
        self
    }

    /// Reemplaza la forma de invocar `git`.
    pub fn con_git(mut self, git: Git) -> Agente {
        self.git = git;
        self
    }

    /// Nivel 1: el plan de aplicar las reglas a la rama `destino`, sin
    /// escribir nada.
    pub fn simular(&self, destino: &str) -> Result<Plan, Error> {
        simulacion::simular(&self.git, &self.raiz, destino, &self.reglas)
    }

    /// Nivel 2: el plan, commiteado en una rama nueva desde un worktree
    /// aislado.
    pub fn preparar(&self, plan: &Plan) -> Result<Preparado, Error> {
        preparacion::preparar(&self.git, &self.raiz, plan)
    }

    /// Nivel 3: la rama destino avanza al commit preparado, si todo coincide
    /// con lo revisado.
    pub fn aplicar(
        &self,
        preparado: &Preparado,
        huella_revisada: &Huella,
        confirmacion: &Confirmacion,
    ) -> Result<Aplicado, Error> {
        aplicacion::aplicar(
            &self.git,
            &self.raiz,
            preparado,
            huella_revisada,
            confirmacion,
        )
    }
}
