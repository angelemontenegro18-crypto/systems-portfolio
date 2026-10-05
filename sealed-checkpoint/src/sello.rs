//! Sellado y apertura, en memoria y sin E/S.
//!
//! ## Formato
//!
//! ```text
//! 0      4   magia "SCKP"
//! 4      1   versión del formato (1)
//! 5      8   generación, u64 little-endian
//! 13     24  nonce aleatorio de XChaCha20-Poly1305
//! 37     n   datos cifrados
//! 37+n   16  etiqueta de autenticación
//! ```
//!
//! Los primeros 13 bytes (el encabezado) viajan en claro — la generación es
//! visible sin la clave — pero van **autenticados** como datos asociados (AAD):
//! cambiar un solo bit del encabezado hace fallar la apertura.
//!
//! El AAD incluye además un **contexto** (en el almacén, el nombre del
//! checkpoint). Un sello hecho para `sesion` no se abre como `config`, aunque
//! alguien renombre el archivo.
//!
//! Un sello que no se puede abrir da siempre el mismo error de autenticación,
//! sea por clave equivocada, por contexto equivocado o por datos alterados: no
//! se le da a un atacante una pista sobre cuál de las tres fue.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::Clave;

const MAGIA: &[u8; 4] = b"SCKP";
const VERSION: u8 = 1;
const LARGO_ENCABEZADO: usize = 4 + 1 + 8;
const LARGO_NONCE: usize = 24;
const LARGO_ETIQUETA: usize = 16;
/// Tamaño mínimo de un sello: encabezado, nonce y etiqueta, sin datos.
pub const LARGO_MINIMO: usize = LARGO_ENCABEZADO + LARGO_NONCE + LARGO_ETIQUETA;

/// Por qué no se pudo sellar o abrir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSello {
    /// Más corto que el mínimo posible.
    Truncado {
        /// Bytes recibidos.
        largo: usize,
    },
    /// No empieza con la magia del formato: no es un sello de este crate.
    NoEsUnSello,
    /// Un formato de una versión que este código no conoce.
    VersionDesconocida(u8),
    /// No abre: clave equivocada, contexto equivocado o datos alterados.
    Autenticacion,
    /// El sistema operativo no pudo dar números aleatorios para el nonce.
    Entropia,
    /// El cifrador rechazó la operación (datos fuera del tamaño admitido).
    Cifrado,
}

impl fmt::Display for ErrorSello {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncado { largo } => write!(
                f,
                "sello truncado: {largo} bytes, el mínimo es {LARGO_MINIMO}"
            ),
            Self::NoEsUnSello => write!(f, "no es un sello de este formato"),
            Self::VersionDesconocida(v) => write!(f, "versión de formato desconocida: {v}"),
            Self::Autenticacion => {
                write!(f, "el sello no se pudo autenticar (clave o contexto equivocados, o datos alterados)")
            }
            Self::Entropia => write!(f, "no hay fuente de aleatoriedad disponible para el nonce"),
            Self::Cifrado => write!(f, "el cifrador rechazó los datos"),
        }
    }
}

impl std::error::Error for ErrorSello {}

/// Un sello abierto. Los datos se borran de la memoria al soltarse.
pub struct Abierto {
    /// La generación con la que se selló.
    pub generacion: u64,
    /// Los datos en claro.
    pub datos: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Abierto {
    // Nunca muestra los datos: podrían ser secretos.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Abierto")
            .field("generacion", &self.generacion)
            .field("bytes", &self.datos.len())
            .finish()
    }
}

fn encabezado(generacion: u64) -> [u8; LARGO_ENCABEZADO] {
    let mut e = [0u8; LARGO_ENCABEZADO];
    e[..4].copy_from_slice(MAGIA);
    e[4] = VERSION;
    e[5..].copy_from_slice(&generacion.to_le_bytes());
    e
}

/// Encabezado + largo del contexto + contexto: todo lo que se autentica sin cifrar.
fn datos_asociados(encabezado: &[u8], contexto: &str) -> Vec<u8> {
    let largo = u32::try_from(contexto.len()).unwrap_or(u32::MAX);
    let mut aad = Vec::with_capacity(encabezado.len() + 4 + contexto.len());
    aad.extend_from_slice(encabezado);
    aad.extend_from_slice(&largo.to_le_bytes());
    aad.extend_from_slice(contexto.as_bytes());
    aad
}

fn cifrador(clave: &Clave) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(Key::from_slice(clave.bytes()))
}

/// Sella `datos` para `contexto` en la `generacion` dada.
pub fn sellar(
    clave: &Clave,
    contexto: &str,
    generacion: u64,
    datos: &[u8],
) -> Result<Vec<u8>, ErrorSello> {
    let encabezado = encabezado(generacion);
    let mut nonce = [0u8; LARGO_NONCE];
    getrandom::getrandom(&mut nonce).map_err(|_| ErrorSello::Entropia)?;

    let aad = datos_asociados(&encabezado, contexto);
    let cifrado = cifrador(clave)
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: datos,
                aad: &aad,
            },
        )
        .map_err(|_| ErrorSello::Cifrado)?;

    let mut sello = Vec::with_capacity(LARGO_ENCABEZADO + LARGO_NONCE + cifrado.len());
    sello.extend_from_slice(&encabezado);
    sello.extend_from_slice(&nonce);
    sello.extend_from_slice(&cifrado);
    Ok(sello)
}

/// Lee la generación del encabezado **sin autenticarla**. Sirve para mostrar;
/// para decidir, hay que abrir el sello.
pub fn generacion_sin_verificar(sello: &[u8]) -> Result<u64, ErrorSello> {
    revisar_encabezado(sello)
}

fn revisar_encabezado(sello: &[u8]) -> Result<u64, ErrorSello> {
    if sello.len() < LARGO_MINIMO {
        return Err(ErrorSello::Truncado { largo: sello.len() });
    }
    if &sello[..4] != MAGIA {
        return Err(ErrorSello::NoEsUnSello);
    }
    if sello[4] != VERSION {
        return Err(ErrorSello::VersionDesconocida(sello[4]));
    }
    let mut g = [0u8; 8];
    g.copy_from_slice(&sello[5..LARGO_ENCABEZADO]);
    Ok(u64::from_le_bytes(g))
}

/// Abre un sello hecho para `contexto`.
pub fn abrir(clave: &Clave, contexto: &str, sello: &[u8]) -> Result<Abierto, ErrorSello> {
    let generacion = revisar_encabezado(sello)?;
    let (encabezado, resto) = sello.split_at(LARGO_ENCABEZADO);
    let (nonce, cifrado) = resto.split_at(LARGO_NONCE);

    let aad = datos_asociados(encabezado, contexto);
    let datos = cifrador(clave)
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: cifrado,
                aad: &aad,
            },
        )
        .map_err(|_| ErrorSello::Autenticacion)?;
    Ok(Abierto {
        generacion,
        datos: Zeroizing::new(datos),
    })
}
