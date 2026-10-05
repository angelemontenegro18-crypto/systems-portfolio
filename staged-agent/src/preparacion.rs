//! Preparar: aplicar el plan en un worktree aislado, sobre una rama nueva.
//!
//! El árbol de trabajo principal y la rama destino no se tocan: todo pasa en
//! un directorio temporal registrado como worktree de `git`. Al terminar se
//! quita el worktree y queda solo una rama con un commit, lista para revisar.
//! Si algo falla a mitad de camino, [`Limpieza`] quita el worktree y la rama
//! que se hayan creado, y nada más.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Error;
use crate::git::Git;
use crate::lectura;
use crate::plan::{huella_de_commit, Huella, Plan};
use crate::rutas;

/// Prefijo de las ramas que crea el agente.
pub const PREFIJO_DE_RAMA: &str = "staged-agent/";

/// Una rama preparada: un solo commit que reproduce un plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preparado {
    pub(crate) rama: String,
    pub(crate) commit: String,
    pub(crate) destino: String,
    pub(crate) huella: Huella,
}

impl Preparado {
    /// Una rama preparada en otra ejecución, por su nombre. [`crate::Agente::aplicar`]
    /// recalcula todo desde el repositorio, así que no hace falta confiar en
    /// estos datos.
    pub fn desde(rama: &str, destino: &str, huella: Huella) -> Preparado {
        Preparado {
            rama: rama.to_string(),
            commit: String::new(),
            destino: destino.to_string(),
            huella,
        }
    }

    /// El nombre de la rama, `staged-agent/<huella corta>`.
    pub fn rama(&self) -> &str {
        &self.rama
    }

    /// El commit preparado; vacío si se reconstruyó con [`Preparado::desde`].
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// La rama destino del plan.
    pub fn destino(&self) -> &str {
        &self.destino
    }

    /// La huella del plan que se preparó.
    pub fn huella(&self) -> Huella {
        self.huella
    }
}

/// Deshace lo que una preparación fallida dejó a medias. Solo toca lo que esta
/// preparación creó: una rama o un directorio que ya existían no son suyos.
struct Limpieza<'a> {
    git: &'a Git,
    raiz: &'a Path,
    directorio: PathBuf,
    rama: String,
    rama_creada: bool,
    directorio_propio: bool,
    worktree_intentado: bool,
}

impl Limpieza<'_> {
    /// Quita el worktree y su directorio. El directorio lo creó esta
    /// preparación con `create_dir`, así que lo que se borra es siempre propio.
    fn quitar_worktree(&mut self) {
        if self.worktree_intentado {
            self.worktree_intentado = false;
            let quitar = [
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                self.directorio.as_os_str(),
            ];
            if self.git.ejecutar(self.raiz, quitar).is_err() {
                // `worktree add` pudo fallar a mitad de camino y dejar un
                // registro sin worktree, que impediría borrar la rama. Se borra
                // el directorio y `prune` olvida el registro. Solo en este caso:
                // `prune` también olvida registros huérfanos ajenos.
                let _ = fs::remove_dir_all(&self.directorio);
                let _ = self.git.ejecutar(self.raiz, ["worktree", "prune"]);
            }
        }
        if self.directorio_propio {
            self.directorio_propio = false;
            let _ = fs::remove_dir_all(&self.directorio);
        }
    }
}

impl Drop for Limpieza<'_> {
    fn drop(&mut self) {
        self.quitar_worktree();
        if self.rama_creada {
            let _ = self
                .git
                .ejecutar(self.raiz, ["branch", "-D", "--", self.rama.as_str()]);
        }
    }
}

/// Crea un directorio temporal nuevo y vacío. `create_dir` falla si el nombre
/// ya existe, así que el directorio que devuelve es de esta preparación y de
/// nadie más; `git worktree add` acepta un directorio vacío.
fn directorio_temporal_nuevo() -> io::Result<PathBuf> {
    static CONTADOR: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let candidato = base.join(format!(
            "staged-agent-{}-{nanos:09}-{n}",
            std::process::id()
        ));
        match fs::create_dir(&candidato) {
            Ok(()) => return Ok(candidato),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no se encontró un directorio temporal libre",
    ))
}

pub(crate) fn preparar(git: &Git, raiz: &Path, plan: &Plan) -> Result<Preparado, Error> {
    if plan.esta_vacio() {
        return Err(Error::PlanVacio);
    }
    // La rama destino sigue donde estaba al simular.
    let actual = lectura::commit_de_rama(git, raiz, &plan.destino)?
        .ok_or_else(|| Error::RamaInexistente(plan.destino.clone()))?;
    if actual != plan.base {
        return Err(Error::DestinoMovido {
            esperado: plan.base.clone(),
            actual,
        });
    }
    // La rama nueva no puede existir: si existe, no es nuestra.
    let rama = format!("{PREFIJO_DE_RAMA}{}", plan.huella.corta());
    if lectura::commit_de_rama(git, raiz, &rama)?.is_some() {
        return Err(Error::RamaExistente(rama));
    }

    let mut limpieza = Limpieza {
        git,
        raiz,
        directorio: directorio_temporal_nuevo()?,
        rama: rama.clone(),
        rama_creada: false,
        directorio_propio: true,
        worktree_intentado: false,
    };
    git.ejecutar(raiz, ["branch", "--", rama.as_str(), plan.base.as_str()])?;
    limpieza.rama_creada = true;
    let agregar = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("--quiet"),
        limpieza.directorio.as_os_str(),
    ];
    limpieza.worktree_intentado = true;
    git.ejecutar(raiz, agregar.into_iter().chain([OsStr::new(rama.as_str())]))?;
    let directorio = limpieza.directorio.clone();

    // Cada archivo: la ruta no sale del worktree, y el contenido es el que el
    // plan espera encontrar.
    for cambio in &plan.cambios {
        let archivo = rutas::resolver_dentro(&directorio, &cambio.ruta)?;
        if fs::read(&archivo)? != cambio.antes {
            return Err(Error::ContenidoInesperado(cambio.ruta.to_string()));
        }
        fs::write(&archivo, &cambio.despues)?;
    }
    // `--literal-pathspecs`: un archivo llamado `a*.txt` es ese archivo, no un
    // patrón que también agregue `ab.txt`.
    let rutas_del_plan = plan.cambios.iter().map(|c| c.ruta.como_str());
    git.ejecutar(
        &directorio,
        ["--literal-pathspecs", "add", "--"]
            .into_iter()
            .chain(rutas_del_plan),
    )?;
    let mensaje = format!(
        "staged-agent: plan {} sobre {}",
        plan.huella.corta(),
        plan.destino
    );
    git.ejecutar(&directorio, ["commit", "--quiet", "-m", mensaje.as_str()])?;

    // El commit tiene que reproducir el plan: un gancho o un filtro podrían
    // haber cambiado lo que se escribió.
    let commit = lectura::commit_de_rama(git, raiz, &rama)?
        .ok_or_else(|| Error::RamaInexistente(rama.clone()))?;
    if huella_de_commit(git, raiz, &plan.destino, &plan.base, &commit)? != Some(plan.huella) {
        return Err(Error::PreparacionInconsistente);
    }

    limpieza.quitar_worktree();
    limpieza.rama_creada = false; // la rama es el entregable: se queda
    Ok(Preparado {
        rama,
        commit,
        destino: plan.destino.clone(),
        huella: plan.huella,
    })
}
