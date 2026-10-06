//! Sellar un paquete, y verificarlo por tipos.
//!
//! Un paquete es la cabecera seguida del contenido, sin nada antes ni después. Del lado que
//! recibe, [`Paquete::leer`] da un `Paquete<Sellado>`: nada de lo que dice está verificado, y
//! por eso **no tiene método para leer el contenido**. [`Paquete::verificar`] lo consume y,
//! si todo cierra, devuelve un `Paquete<Verificado>`, el único que lo entrega.
//!
//! ```compile_fail,E0599
//! use state_handoff::paquete::{sellar, Paquete};
//!
//! let mut bytes = [0u8; 64];
//! let n = sellar(&[1, 2, 3], 1, 2, 5, &mut bytes).unwrap();
//! let sellado = Paquete::leer(&bytes[..n]).unwrap();
//! // error[E0599]: un paquete sin verificar no tiene `contenido`.
//! let _ = sellado.contenido();
//! ```
//!
//! Y un `Paquete<Verificado>` no se puede fabricar sin pasar por `verificar`:
//!
//! ```compile_fail,E0451
//! use core::marker::PhantomData;
//! use state_handoff::cabecera::{Cabecera, FIRMA, VERSION};
//! use state_handoff::paquete::{Paquete, Verificado};
//!
//! let cabecera = Cabecera {
//!     firma: FIRMA,
//!     version: VERSION,
//!     origen: 1,
//!     destino: 2,
//!     epoca: 9,
//!     largo: 0,
//!     crc_contenido: 0,
//!     crc_cabecera: 0,
//! };
//! // error[E0451]: los campos de `Paquete` son privados.
//! let falso: Paquete<'_, Verificado> = Paquete { bytes: &[], cabecera, estado: PhantomData };
//! ```

use core::cmp::Ordering;
use core::marker::PhantomData;

use crate::cabecera::{Cabecera, DESPLAZAMIENTO_CRC_CABECERA, FIRMA, LARGO_CABECERA, VERSION};
use crate::crc32::crc32;
use crate::le::{Escritor, Lector};
use crate::Error;

/// El estado de un paquete recién leído: nada de lo que dice está verificado.
#[derive(Debug)]
pub enum Sellado {}

/// El estado de un paquete que pasó todas las verificaciones.
#[derive(Debug)]
pub enum Verificado {}

/// Un paquete de traspaso, en el estado `Estado`.
#[derive(Debug)]
pub struct Paquete<'a, Estado> {
    bytes: &'a [u8],
    cabecera: Cabecera,
    estado: PhantomData<Estado>,
}

impl<'a> Paquete<'a, Sellado> {
    /// Lee la cabecera de `bytes`, sin verificar nada. Si no alcanzan para una cabecera,
    /// [`Error::Truncado`].
    pub fn leer(bytes: &'a [u8]) -> Result<Paquete<'a, Sellado>, Error> {
        let cabecera = Cabecera::leer_campos(&mut Lector::nuevo(bytes))?;
        Ok(Paquete {
            bytes,
            cabecera,
            estado: PhantomData,
        })
    }

    /// Verifica el paquete para el nodo `propio`, en este orden: firma, versión, CRC de la
    /// cabecera, largo exacto (ni un byte de menos ni de más), CRC del contenido y destino.
    /// El primer fallo es el error.
    pub fn verificar(self, propio: u16) -> Result<Paquete<'a, Verificado>, Error> {
        let c = &self.cabecera;
        if c.firma != FIRMA {
            return Err(Error::FirmaAjena);
        }
        if c.version != VERSION {
            return Err(Error::VersionDesconocida(c.version));
        }
        let cubierto = self
            .bytes
            .get(..DESPLAZAMIENTO_CRC_CABECERA)
            .ok_or(Error::Truncado)?;
        if crc32(cubierto) != c.crc_cabecera {
            return Err(Error::CabeceraCorrupta);
        }
        let largo = usize::try_from(c.largo).map_err(|_| Error::NoCabe)?;
        let contenido = self.bytes.get(LARGO_CABECERA..).ok_or(Error::Truncado)?;
        match contenido.len().cmp(&largo) {
            Ordering::Less => return Err(Error::Truncado),
            Ordering::Greater => return Err(Error::Sobrante),
            Ordering::Equal => {}
        }
        if crc32(contenido) != c.crc_contenido {
            return Err(Error::ContenidoCorrupto);
        }
        if c.destino != propio {
            return Err(Error::DestinoAjeno(c.destino));
        }
        Ok(Paquete {
            bytes: self.bytes,
            cabecera: self.cabecera,
            estado: PhantomData,
        })
    }
}

impl<'a> Paquete<'a, Verificado> {
    /// El contenido, ya verificado.
    pub fn contenido(&self) -> &'a [u8] {
        self.bytes.get(LARGO_CABECERA..).unwrap_or(&[])
    }

    /// La cabecera, ya verificada.
    pub fn cabecera(&self) -> &Cabecera {
        &self.cabecera
    }

    /// La época del estado que trae.
    pub fn epoca(&self) -> u64 {
        self.cabecera.epoca
    }

    /// El nodo que lo selló.
    pub fn origen(&self) -> u16 {
        self.cabecera.origen
    }
}

/// Sella `contenido` en un paquete de `origen` para `destino`, en la época `epoca`, y lo
/// escribe al principio de `salida`. Devuelve el largo del paquete: la cabecera más el
/// contenido.
pub fn sellar(
    contenido: &[u8],
    origen: u16,
    destino: u16,
    epoca: u64,
    salida: &mut [u8],
) -> Result<usize, Error> {
    let largo = u32::try_from(contenido.len()).map_err(|_| Error::NoCabe)?;
    let total = LARGO_CABECERA
        .checked_add(contenido.len())
        .ok_or(Error::NoCabe)?;
    let salida = salida.get_mut(..total).ok_or(Error::SalidaCorta)?;

    let mut cabecera = Cabecera {
        firma: FIRMA,
        version: VERSION,
        origen,
        destino,
        epoca,
        largo,
        crc_contenido: crc32(contenido),
        crc_cabecera: 0,
    };
    // Primero la cabecera sin su CRC, para calcularlo sobre los bytes que de verdad viajan.
    let mut provisoria = [0u8; LARGO_CABECERA];
    cabecera.escribir_campos(&mut Escritor::nuevo(&mut provisoria))?;
    let cubierto = provisoria
        .get(..DESPLAZAMIENTO_CRC_CABECERA)
        .ok_or(Error::SalidaCorta)?;
    cabecera.crc_cabecera = crc32(cubierto);

    let mut escritor = Escritor::nuevo(salida);
    cabecera.escribir_campos(&mut escritor)?;
    escritor.bytes(contenido)?;
    Ok(total)
}
