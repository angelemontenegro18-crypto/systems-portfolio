//! Cliente síncrono sobre cualquier flujo de bytes.
//!
//! [`Cliente`] es genérico sobre `Read + Write`: en producción es un
//! `TcpStream` con timeouts, en los tests puede ser cualquier cosa. Lee del
//! socket exactamente lo que la cabecera anuncia — nunca "lo que haya" — para
//! que una respuesta vieja o ajena no se mezcle con la siguiente.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::trama::{self, ErrorTrama, Funcion, Peticion, LARGO_MBAP};

/// Lo que puede fallar al leer un equipo.
#[derive(Debug)]
pub enum ErrorModbus {
    /// La petición no es válida (cantidad o rango); no se llegó a enviar.
    PeticionInvalida(&'static str),
    /// Falla de red, incluido el timeout (`ErrorKind::WouldBlock`/`TimedOut`).
    Io(io::Error),
    /// Llegó una respuesta, pero no corresponde a lo pedido o es una excepción.
    Trama(ErrorTrama),
}

impl ErrorModbus {
    /// `true` si la falla fue que el equipo no respondió a tiempo.
    pub fn es_timeout(&self) -> bool {
        matches!(self, Self::Io(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
    }
}

impl fmt::Display for ErrorModbus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeticionInvalida(m) => write!(f, "petición inválida: {m}"),
            Self::Io(e) if self.es_timeout() => write!(f, "el equipo no respondió a tiempo ({e})"),
            Self::Io(e) => write!(f, "falla de red: {e}"),
            Self::Trama(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ErrorModbus {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Trama(e) => Some(e),
            Self::PeticionInvalida(_) => None,
        }
    }
}

impl From<io::Error> for ErrorModbus {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ErrorTrama> for ErrorModbus {
    fn from(e: ErrorTrama) -> Self {
        Self::Trama(e)
    }
}

/// Cliente de lectura para una unidad Modbus.
pub struct Cliente<S: Read + Write> {
    flujo: S,
    unidad: u8,
    siguiente_transaccion: u16,
}

impl Cliente<TcpStream> {
    /// Se conecta por TCP. `tiempo_conexion` acota el `connect`;
    /// `tiempo_peticion` acota cada lectura y escritura en el socket.
    pub fn conectar(
        direccion: SocketAddr,
        unidad: u8,
        tiempo_conexion: Duration,
        tiempo_peticion: Duration,
    ) -> Result<Self, ErrorModbus> {
        let flujo = TcpStream::connect_timeout(&direccion, tiempo_conexion)?;
        flujo.set_read_timeout(Some(tiempo_peticion))?;
        flujo.set_write_timeout(Some(tiempo_peticion))?;
        flujo.set_nodelay(true)?;
        Ok(Self::sobre(flujo, unidad))
    }
}

impl<S: Read + Write> Cliente<S> {
    /// Envuelve un flujo ya abierto.
    pub fn sobre(flujo: S, unidad: u8) -> Self {
        Self {
            flujo,
            unidad,
            siguiente_transaccion: 1,
        }
    }

    /// Unidad a la que habla este cliente.
    pub fn unidad(&self) -> u8 {
        self.unidad
    }

    /// Lee `cantidad` registros desde `direccion`.
    pub fn leer(
        &mut self,
        funcion: Funcion,
        direccion: u16,
        cantidad: u16,
    ) -> Result<Vec<u16>, ErrorModbus> {
        let transaccion = self.siguiente_transaccion;
        self.siguiente_transaccion = self.siguiente_transaccion.wrapping_add(1);

        let peticion = Peticion::nueva(transaccion, self.unidad, funcion, direccion, cantidad)
            .map_err(ErrorModbus::PeticionInvalida)?;

        self.flujo.write_all(&peticion.codificar())?;
        self.flujo.flush()?;

        let mut cabecera = [0u8; LARGO_MBAP];
        self.flujo.read_exact(&mut cabecera)?;
        let pdu = trama::largo_pdu(&cabecera)?;

        let mut respuesta = Vec::with_capacity(LARGO_MBAP + pdu);
        respuesta.extend_from_slice(&cabecera);
        respuesta.resize(LARGO_MBAP + pdu, 0);
        self.flujo.read_exact(&mut respuesta[LARGO_MBAP..])?;

        Ok(trama::decodificar_respuesta(&peticion, &respuesta)?)
    }

    /// Atajo para registros de retención.
    pub fn leer_retencion(
        &mut self,
        direccion: u16,
        cantidad: u16,
    ) -> Result<Vec<u16>, ErrorModbus> {
        self.leer(Funcion::RegistrosRetencion, direccion, cantidad)
    }

    /// Atajo para registros de entrada.
    pub fn leer_entrada(&mut self, direccion: u16, cantidad: u16) -> Result<Vec<u16>, ErrorModbus> {
        self.leer(Funcion::RegistrosEntrada, direccion, cantidad)
    }
}
