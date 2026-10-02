//! Checkpoints sellados en disco.
//!
//! ## Escritura atómica y durable
//!
//! 1. Se escribe un archivo temporal **nuevo** en el mismo directorio
//!    (`create_new`: nunca se reutiliza ni se sigue un enlace que ya estaba ahí).
//! 2. `sync_all` sobre el temporal: los bytes están en disco.
//! 3. `rename` sobre el destino: el cambio es atómico, así que un lector ve la
//!    versión vieja o la nueva, nunca una mezcla.
//! 4. `sync_all` sobre el **directorio**. Sin este paso el `rename` puede
//!    perderse ante un corte de luz aunque los bytes del archivo ya estén
//!    escritos: es la entrada de directorio la que tiene que llegar a disco.
//!
//! Si algo falla antes del `rename`, el temporal se borra y el checkpoint
//! anterior queda intacto.
//!
//! ## Protección contra retroceso
//!
//! Un checkpoint viejo pero auténtico sigue abriendo bien. Si alguien
//! reemplaza el archivo por una copia anterior, el sello no lo delata. Por eso:
//!
//! - [`Almacen::guardar`] rechaza una generación menor o igual a la existente;
//! - [`Almacen::cargar_desde`] rechaza una generación menor a la mínima que el
//!   llamador ya conoce (la última que vio, guardada en otro lado).

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sello::{self, Abierto, ErrorSello};
use crate::Clave;

const EXTENSION: &str = "sck";

/// Qué puede fallar al guardar o cargar.
#[derive(Debug)]
pub enum ErrorAlmacen {
    /// El nombre no cumple `[a-z0-9_-]{1,64}`.
    NombreInvalido(String),
    /// Falla del sistema de archivos.
    Io(io::Error),
    /// El archivo existe pero no se pudo abrir el sello.
    Sello(ErrorSello),
    /// Se intentó guardar una generación que no avanza.
    GeneracionNoAvanza {
        /// La que ya está guardada.
        existente: u64,
        /// La que se intentó guardar.
        pedida: u64,
    },
    /// El archivo tiene una generación anterior a la mínima conocida.
    Retroceso {
        /// La que está en disco.
        encontrada: u64,
        /// La mínima que se esperaba.
        minima: u64,
    },
}

impl fmt::Display for ErrorAlmacen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NombreInvalido(n) => write!(f, "nombre inválido `{n}`: se admite [a-z0-9_-], de 1 a 64"),
            Self::Io(e) => write!(f, "falla de disco: {e}"),
            Self::Sello(e) => write!(f, "{e}"),
            Self::GeneracionNoAvanza { existente, pedida } => {
                write!(f, "la generación {pedida} no avanza sobre la guardada ({existente})")
            }
            Self::Retroceso { encontrada, minima } => {
                write!(f, "el checkpoint retrocedió: generación {encontrada}, la mínima conocida es {minima}")
            }
        }
    }
}

impl std::error::Error for ErrorAlmacen {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Sello(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ErrorAlmacen {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Un directorio de checkpoints sellados con una clave.
#[derive(Debug)]
pub struct Almacen {
    dir: PathBuf,
    clave: Clave,
}

/// Distingue temporales de escrituras sucesivas dentro del mismo proceso.
static CONTADOR: AtomicU64 = AtomicU64::new(0);

fn validar_nombre(nombre: &str) -> Result<(), ErrorAlmacen> {
    let ok = (1..=64).contains(&nombre.len())
        && nombre.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ErrorAlmacen::NombreInvalido(nombre.to_string()))
    }
}

impl Almacen {
    /// Usa `dir` como almacén; lo crea si no existe.
    pub fn abrir(dir: impl Into<PathBuf>, clave: Clave) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir, clave })
    }

    /// Ruta del archivo de un checkpoint.
    pub fn ruta_de(&self, nombre: &str) -> Result<PathBuf, ErrorAlmacen> {
        validar_nombre(nombre)?;
        Ok(self.dir.join(format!("{nombre}.{EXTENSION}")))
    }

    /// Sella y guarda. Rechaza una generación que no avance sobre la guardada.
    ///
    /// Si el archivo existente está corrupto o no abre con esta clave, se
    /// reemplaza: no se puede exigir que avance sobre algo que no se puede leer.
    pub fn guardar(&self, nombre: &str, generacion: u64, datos: &[u8]) -> Result<(), ErrorAlmacen> {
        let destino = self.ruta_de(nombre)?;
        if let Some(existente) = self.leer(nombre, &destino).ok().flatten() {
            if generacion <= existente.generacion {
                return Err(ErrorAlmacen::GeneracionNoAvanza { existente: existente.generacion, pedida: generacion });
            }
        }
        let sellado = sello::sellar(&self.clave, nombre, generacion, datos).map_err(ErrorAlmacen::Sello)?;
        escribir_atomico(&self.dir, nombre, &destino, &sellado)?;
        Ok(())
    }

    /// Carga el último checkpoint guardado, o `None` si no hay.
    pub fn cargar(&self, nombre: &str) -> Result<Option<Abierto>, ErrorAlmacen> {
        let ruta = self.ruta_de(nombre)?;
        self.leer(nombre, &ruta)
    }

    /// Como [`Almacen::cargar`], pero rechaza un checkpoint anterior a `minima`.
    pub fn cargar_desde(&self, nombre: &str, minima: u64) -> Result<Option<Abierto>, ErrorAlmacen> {
        match self.cargar(nombre)? {
            Some(a) if a.generacion < minima => Err(ErrorAlmacen::Retroceso { encontrada: a.generacion, minima }),
            otro => Ok(otro),
        }
    }

    fn leer(&self, nombre: &str, ruta: &Path) -> Result<Option<Abierto>, ErrorAlmacen> {
        let bytes = match fs::read(ruta) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        // El nombre es el contexto del sello: un archivo renombrado no abre.
        sello::abrir(&self.clave, nombre, &bytes).map(Some).map_err(ErrorAlmacen::Sello)
    }
}

fn escribir_atomico(dir: &Path, nombre: &str, destino: &Path, bytes: &[u8]) -> io::Result<()> {
    let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
    let temporal = dir.join(format!(".{nombre}.{}.{n}.tmp", std::process::id()));

    let resultado = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&temporal)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temporal, destino)?;
        sincronizar_directorio(dir)
    })();

    if resultado.is_err() {
        // Si el rename ya ocurrió, el temporal no existe y esto no hace nada.
        let _ = fs::remove_file(&temporal);
    }
    resultado
}

/// Hace durable la entrada de directorio del `rename`.
#[cfg(unix)]
fn sincronizar_directorio(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// La biblioteca estándar no permite abrir un directorio para sincronizarlo
/// fuera de Unix. En Windows, `rename` sobre un archivo existente usa
/// `MoveFileEx` con reemplazo, y la durabilidad del directorio depende del
/// sistema de archivos.
#[cfg(not(unix))]
fn sincronizar_directorio(_dir: &Path) -> io::Result<()> {
    Ok(())
}
