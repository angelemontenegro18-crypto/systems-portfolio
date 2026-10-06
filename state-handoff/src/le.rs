//! Escritura y lectura little-endian explícitas.
//!
//! [`Escritor`] y [`Lector`] avanzan un cursor sobre un búfer y nunca indexan: cada acceso
//! pasa por `get`/`get_mut`, y lo que no cabe o falta vuelve como error. El orden de bytes es
//! siempre little-endian, escrito a mano con `to_le_bytes`/`from_le_bytes`, sin depender del
//! procesador.

use crate::Error;

/// Escribe en un búfer, de adelante hacia atrás.
#[derive(Debug)]
pub struct Escritor<'a> {
    salida: &'a mut [u8],
    posicion: usize,
}

impl<'a> Escritor<'a> {
    /// Un escritor al principio de `salida`.
    pub fn nuevo(salida: &'a mut [u8]) -> Escritor<'a> {
        Escritor {
            salida,
            posicion: 0,
        }
    }

    /// Cuántos bytes lleva escritos.
    pub fn posicion(&self) -> usize {
        self.posicion
    }

    /// Escribe `datos` tal cual. Si no caben, [`Error::SalidaCorta`] y no escribe nada.
    pub fn bytes(&mut self, datos: &[u8]) -> Result<(), Error> {
        let fin = self
            .posicion
            .checked_add(datos.len())
            .ok_or(Error::SalidaCorta)?;
        let destino = self
            .salida
            .get_mut(self.posicion..fin)
            .ok_or(Error::SalidaCorta)?;
        destino.copy_from_slice(datos);
        self.posicion = fin;
        Ok(())
    }

    /// Un `u8`.
    pub fn u8(&mut self, valor: u8) -> Result<(), Error> {
        self.bytes(&[valor])
    }

    /// Un `u16`, en little-endian.
    pub fn u16_le(&mut self, valor: u16) -> Result<(), Error> {
        self.bytes(&valor.to_le_bytes())
    }

    /// Un `u32`, en little-endian.
    pub fn u32_le(&mut self, valor: u32) -> Result<(), Error> {
        self.bytes(&valor.to_le_bytes())
    }

    /// Un `u64`, en little-endian.
    pub fn u64_le(&mut self, valor: u64) -> Result<(), Error> {
        self.bytes(&valor.to_le_bytes())
    }
}

/// Lee de un búfer, de adelante hacia atrás.
#[derive(Debug, Clone)]
pub struct Lector<'a> {
    entrada: &'a [u8],
    posicion: usize,
}

impl<'a> Lector<'a> {
    /// Un lector al principio de `entrada`.
    pub fn nuevo(entrada: &'a [u8]) -> Lector<'a> {
        Lector {
            entrada,
            posicion: 0,
        }
    }

    /// Cuántos bytes lleva leídos.
    pub fn posicion(&self) -> usize {
        self.posicion
    }

    /// Lo que queda por leer.
    pub fn resto(&self) -> &'a [u8] {
        self.entrada.get(self.posicion..).unwrap_or(&[])
    }

    /// Los próximos `N` bytes. Si no hay tantos, [`Error::Truncado`] y el cursor no se mueve.
    pub fn bytes<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let fin = self.posicion.checked_add(N).ok_or(Error::Truncado)?;
        let trozo = self
            .entrada
            .get(self.posicion..fin)
            .ok_or(Error::Truncado)?;
        let arreglo = <[u8; N]>::try_from(trozo).map_err(|_| Error::Truncado)?;
        self.posicion = fin;
        Ok(arreglo)
    }

    /// Un `u8`.
    pub fn u8(&mut self) -> Result<u8, Error> {
        self.bytes::<1>().map(|[b]| b)
    }

    /// Un `u16`, en little-endian.
    pub fn u16_le(&mut self) -> Result<u16, Error> {
        self.bytes().map(u16::from_le_bytes)
    }

    /// Un `u32`, en little-endian.
    pub fn u32_le(&mut self) -> Result<u32, Error> {
        self.bytes().map(u32::from_le_bytes)
    }

    /// Un `u64`, en little-endian.
    pub fn u64_le(&mut self) -> Result<u64, Error> {
        self.bytes().map(u64::from_le_bytes)
    }
}

/// Un campo de ancho fijo que se escribe y se lee en little-endian. Lo usa
/// [`formato_de_cabecera!`](crate::formato_de_cabecera) para generar la codificación de la
/// cabecera a partir de la lista de sus campos.
pub trait CampoLe: Sized {
    /// Escribe el campo.
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error>;
    /// Lee el campo.
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error>;
}

impl CampoLe for u8 {
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error> {
        escritor.u8(*self)
    }
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error> {
        lector.u8()
    }
}

impl CampoLe for u16 {
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error> {
        escritor.u16_le(*self)
    }
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error> {
        lector.u16_le()
    }
}

impl CampoLe for u32 {
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error> {
        escritor.u32_le(*self)
    }
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error> {
        lector.u32_le()
    }
}

impl CampoLe for u64 {
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error> {
        escritor.u64_le(*self)
    }
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error> {
        lector.u64_le()
    }
}

impl<const N: usize> CampoLe for [u8; N] {
    fn escribir(&self, escritor: &mut Escritor<'_>) -> Result<(), Error> {
        escritor.bytes(self)
    }
    fn leer(lector: &mut Lector<'_>) -> Result<Self, Error> {
        lector.bytes()
    }
}
