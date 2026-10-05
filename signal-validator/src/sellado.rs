//! La muestra reservada: se sella antes de mirar nada y se abre una sola vez.
//!
//! Una muestra de confirmación que se consulta dos veces deja de confirmar:
//! la segunda mirada ya está condicionada por la primera. [`MuestraSellada`]
//! hace que eso no compile:
//!
//! - [`abrir`](MuestraSellada::abrir) toma la muestra **por valor**: después
//!   de abrirla, ya no existe.
//! - No implementa `Clone`, así que tampoco se puede copiar antes de abrir.
//! - Sus datos no se pueden leer sin abrirla, y su `Debug` no los muestra.
//!
//! Al sellarla se calcula un [`Compromiso`]: el SHA-256 de su contenido. Si se
//! anota antes del análisis, cualquiera puede comprobar después que la muestra
//! que se abrió es la misma que se reservó.
//!
//! Lo que el sistema de tipos no ve es otra ejecución del programa. Para eso
//! está [`abrir_con_marca`](MuestraSellada::abrir_con_marca), que deja un
//! archivo testigo y se niega a abrir si ya existe. **Es disciplina, no
//! seguridad**: alguien decidido a mirar dos veces puede borrar la marca, o
//! volver a sellar los datos de origen. Lo que se evita es mirar dos veces sin
//! darse cuenta.
//!
//! ```compile_fail,E0382
//! use signal_validator::sellado::MuestraSellada;
//!
//! let muestra = MuestraSellada::sellar(vec![1.0, 2.0, 3.0]);
//! let primera = muestra.abrir();
//! let segunda = muestra.abrir(); // error[E0382]: la muestra ya se movió
//! ```
//!
//! ```compile_fail,E0599
//! use signal_validator::sellado::MuestraSellada;
//!
//! let muestra = MuestraSellada::sellar(vec![1.0, 2.0, 3.0]);
//! let copia = muestra.clone(); // error[E0599]: no implementa Clone
//! ```
//!
//! ```compile_fail,E0616
//! use signal_validator::sellado::MuestraSellada;
//!
//! let muestra = MuestraSellada::sellar(vec![1.0, 2.0, 3.0]);
//! let espiar = &muestra.datos; // error[E0616]: el campo es privado
//! ```

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::particion::{exigir_orden, ErrorParticion, Intervalo};
use crate::sha256::sha256;

/// Prefijo de todo lo que se resume: un compromiso de este crate no puede
/// coincidir con el SHA-256 de otra cosa que empiece igual.
const DOMINIO: &[u8] = b"signal-validator/muestra-sellada/v1\0";

/// Codificación canónica a bytes, la que entra en el [`Compromiso`].
///
/// Los números van en little-endian; los `f64`, bit a bit (`0.0` y `-0.0` son
/// distintos); las secuencias, precedidas por su largo como `u64`.
pub trait Codificable {
    /// Agrega la codificación de `self` al final de `salida`.
    fn codificar(&self, salida: &mut Vec<u8>);
}

impl Codificable for f64 {
    fn codificar(&self, salida: &mut Vec<u8>) {
        salida.extend_from_slice(&self.to_bits().to_le_bytes());
    }
}

impl Codificable for u64 {
    fn codificar(&self, salida: &mut Vec<u8>) {
        salida.extend_from_slice(&self.to_le_bytes());
    }
}

impl Codificable for i64 {
    fn codificar(&self, salida: &mut Vec<u8>) {
        salida.extend_from_slice(&self.to_le_bytes());
    }
}

impl Codificable for u32 {
    fn codificar(&self, salida: &mut Vec<u8>) {
        salida.extend_from_slice(&self.to_le_bytes());
    }
}

impl<T: Codificable> Codificable for [T] {
    fn codificar(&self, salida: &mut Vec<u8>) {
        (self.len() as u64).codificar(salida);
        for valor in self {
            valor.codificar(salida);
        }
    }
}

impl<T: Codificable> Codificable for Vec<T> {
    fn codificar(&self, salida: &mut Vec<u8>) {
        self.as_slice().codificar(salida);
    }
}

impl<A: Codificable, B: Codificable> Codificable for (A, B) {
    fn codificar(&self, salida: &mut Vec<u8>) {
        self.0.codificar(salida);
        self.1.codificar(salida);
    }
}

impl<A: Codificable, B: Codificable, C: Codificable> Codificable for (A, B, C) {
    fn codificar(&self, salida: &mut Vec<u8>) {
        self.0.codificar(salida);
        self.1.codificar(salida);
        self.2.codificar(salida);
    }
}

/// SHA-256 del contenido de una muestra, calculado al sellarla.
///
/// Compromete pero no oculta: quien tenga los datos puede recalcularlo, que es
/// justamente lo que se busca.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Compromiso([u8; 32]);

impl Compromiso {
    /// El compromiso que tendría una muestra con estos datos.
    pub fn de<T: Codificable>(datos: &[T]) -> Compromiso {
        let mut bytes = DOMINIO.to_vec();
        datos.codificar(&mut bytes);
        Compromiso(sha256(&bytes))
    }

    /// Los 32 bytes del resumen.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Compromiso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Compromiso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Compromiso({self})")
    }
}

/// Muestra reservada que se abre una sola vez. Ver el [módulo](self).
pub struct MuestraSellada<T> {
    datos: Vec<T>,
    compromiso: Compromiso,
}

impl<T: Codificable> MuestraSellada<T> {
    /// Sella los datos y calcula su compromiso.
    pub fn sellar(datos: Vec<T>) -> MuestraSellada<T> {
        let compromiso = Compromiso::de(&datos);
        MuestraSellada { datos, compromiso }
    }
}

impl<T> MuestraSellada<T> {
    /// El compromiso calculado al sellar. Se puede leer sin abrir.
    pub fn compromiso(&self) -> Compromiso {
        self.compromiso
    }

    /// Cuántas observaciones hay. Se puede leer sin abrir.
    pub fn len(&self) -> usize {
        self.datos.len()
    }

    /// `true` si la muestra no tiene observaciones.
    pub fn is_empty(&self) -> bool {
        self.datos.is_empty()
    }

    /// Abre la muestra. La consume: no hay segunda vez.
    pub fn abrir(self) -> Vec<T> {
        self.datos
    }

    /// Abre la muestra solo si `marca` no existe, y deja la marca creada con el
    /// compromiso adentro.
    ///
    /// La marca se crea con `create_new`, que falla si el archivo ya existe:
    /// dos ejecuciones no pueden pasar las dos, ni siquiera a la vez. Si algo
    /// falla, la muestra vuelve sellada dentro del error
    /// ([`ErrorApertura::recuperar`]): un error nunca destruye datos.
    pub fn abrir_con_marca(self, marca: impl AsRef<Path>) -> Result<Vec<T>, ErrorApertura<T>> {
        let marca = marca.as_ref();
        let mut archivo = match OpenOptions::new().write(true).create_new(true).open(marca) {
            Ok(archivo) => archivo,
            Err(e) => {
                let motivo = if e.kind() == io::ErrorKind::AlreadyExists {
                    MotivoRechazo::YaAbierta(marca.to_path_buf())
                } else {
                    MotivoRechazo::Io(e)
                };
                return Err(ErrorApertura {
                    muestra: self,
                    motivo,
                });
            }
        };
        let escrito = archivo
            .write_all(self.compromiso.to_string().as_bytes())
            .and_then(|()| archivo.sync_all());
        if let Err(e) = escrito {
            // La muestra no se abrió, así que la marca tampoco debe quedar.
            drop(archivo);
            let _ = fs::remove_file(marca);
            return Err(ErrorApertura {
                muestra: self,
                motivo: MotivoRechazo::Io(e),
            });
        }
        Ok(self.datos)
    }
}

impl<T> fmt::Debug for MuestraSellada<T> {
    /// Muestra el tamaño y el compromiso, nunca los datos.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MuestraSellada")
            .field("observaciones", &self.datos.len())
            .field("compromiso", &self.compromiso)
            .finish_non_exhaustive()
    }
}

/// Por qué [`abrir_con_marca`](MuestraSellada::abrir_con_marca) no abrió.
#[derive(Debug)]
pub enum MotivoRechazo {
    /// La marca ya existe: esta muestra (u otra con la misma marca) ya se abrió.
    YaAbierta(PathBuf),
    /// No se pudo crear o escribir la marca.
    Io(io::Error),
}

/// Error de [`abrir_con_marca`](MuestraSellada::abrir_con_marca). Lleva la
/// muestra, todavía sellada.
pub struct ErrorApertura<T> {
    muestra: MuestraSellada<T>,
    motivo: MotivoRechazo,
}

impl<T> ErrorApertura<T> {
    /// Por qué no se abrió.
    pub fn motivo(&self) -> &MotivoRechazo {
        &self.motivo
    }

    /// Devuelve la muestra, todavía sellada.
    pub fn recuperar(self) -> MuestraSellada<T> {
        self.muestra
    }
}

impl<T> fmt::Debug for ErrorApertura<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorApertura")
            .field("muestra", &self.muestra)
            .field("motivo", &self.motivo)
            .finish()
    }
}

impl<T> fmt::Display for ErrorApertura<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.motivo {
            MotivoRechazo::YaAbierta(marca) => {
                write!(
                    f,
                    "la muestra ya se abrió: existe la marca {}",
                    marca.display()
                )
            }
            MotivoRechazo::Io(e) => write!(f, "no se pudo dejar la marca de apertura: {e}"),
        }
    }
}

impl<T> std::error::Error for ErrorApertura<T> {}

/// Diseño y reserva, separados en el tiempo.
#[derive(Debug)]
pub struct Separacion<T> {
    /// Observaciones para el diseño, sin las que se purgaron.
    pub diseno: Vec<T>,
    /// La reserva, sellada.
    pub reserva: MuestraSellada<T>,
    /// Observaciones del diseño descartadas por cruzarse con la reserva.
    pub purgadas: usize,
}

/// Separa las observaciones en diseño (`..corte`) y reserva sellada
/// (`corte..`), y purga del diseño las que comparten información con la
/// reserva.
///
/// Se purga del lado del diseño para que la reserva quede intacta: su
/// compromiso cubre exactamente lo que se reservó. `intervalos` va en el mismo
/// orden que `datos` y en orden temporal.
pub fn separar<T: Codificable>(
    datos: Vec<T>,
    intervalos: &[Intervalo],
    corte: usize,
) -> Result<Separacion<T>, ErrorParticion> {
    if datos.len() != intervalos.len() {
        return Err(ErrorParticion::LargosDistintos {
            observaciones: datos.len(),
            intervalos: intervalos.len(),
        });
    }
    if corte == 0 || corte >= datos.len() {
        return Err(ErrorParticion::CorteInvalido {
            corte,
            observaciones: datos.len(),
        });
    }
    exigir_orden(intervalos)?;

    // El primer instante del que depende cualquier observación de la reserva.
    let inicio_reserva = intervalos[corte..]
        .iter()
        .map(Intervalo::inicio)
        .min()
        .unwrap_or(u64::MAX);
    let mut diseno = datos;
    let reservados = diseno.split_off(corte);
    let antes = diseno.len();
    let mut i = 0;
    diseno.retain(|_| {
        let conserva = intervalos[i].fin() < inicio_reserva;
        i += 1;
        conserva
    });
    let purgadas = antes - diseno.len();
    Ok(Separacion {
        diseno,
        reserva: MuestraSellada::sellar(reservados),
        purgadas,
    })
}
