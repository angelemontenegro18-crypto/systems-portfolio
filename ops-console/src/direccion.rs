//! Dónde escucha el servidor: solo en la propia máquina, y el tipo lo garantiza.
//!
//! [`Servidor::escuchar`](crate::servidor::Servidor::escuchar) no recibe un `SocketAddr`
//! cualquiera sino una [`DireccionLocal`], y una `DireccionLocal` solo existe si su IP es de
//! loopback: `127.0.0.0/8` o `::1`. No hay otra forma de construirla.
//!
//! ```compile_fail,E0451
//! use std::net::SocketAddr;
//! use ops_console::direccion::DireccionLocal;
//!
//! // error[E0451]: el campo es privado; solo `DireccionLocal::nueva` la construye.
//! let abierta = DireccionLocal { socket: SocketAddr::from(([0, 0, 0, 0], 8750)) };
//! ```

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Una dirección de escucha de **loopback**: `127.0.0.0/8` o `::1`, con su puerto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DireccionLocal {
    socket: SocketAddr,
}

/// Por qué una dirección no sirve para escuchar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDireccion {
    /// No es `IP:puerto` ni `localhost:puerto`.
    Ilegible(String),
    /// Es una IP, pero no de loopback: el servidor quedaría expuesto a la red.
    NoLocal(IpAddr),
}

impl fmt::Display for ErrorDireccion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorDireccion::Ilegible(texto) => write!(
                f,
                "«{texto}» no es una dirección: se espera IP:puerto (127.0.0.1:8750, [::1]:8750) \
                 o localhost:puerto"
            ),
            ErrorDireccion::NoLocal(ip) => write!(
                f,
                "{ip} no es de esta máquina: la consola solo escucha en loopback \
                 (127.0.0.0/8 o ::1)"
            ),
        }
    }
}

impl std::error::Error for ErrorDireccion {}

impl DireccionLocal {
    /// La dirección, si su IP es de loopback.
    pub fn nueva(socket: SocketAddr) -> Result<DireccionLocal, ErrorDireccion> {
        if socket.ip().is_loopback() {
            Ok(DireccionLocal { socket })
        } else {
            Err(ErrorDireccion::NoLocal(socket.ip()))
        }
    }

    /// Lee `127.0.0.1:8750`, `[::1]:8750` o `localhost:8750`. El nombre `localhost` se toma
    /// como `127.0.0.1` **sin consultar ningún resolvedor**; cualquier otro nombre se rechaza,
    /// porque un nombre puede resolver a cualquier parte.
    pub fn desde_texto(texto: &str) -> Result<DireccionLocal, ErrorDireccion> {
        if let Some((nombre, puerto)) = texto.rsplit_once(':') {
            if nombre.eq_ignore_ascii_case("localhost") {
                let puerto: u16 = puerto
                    .parse()
                    .map_err(|_| ErrorDireccion::Ilegible(texto.to_string()))?;
                return DireccionLocal::nueva(SocketAddr::from((Ipv4Addr::LOCALHOST, puerto)));
            }
        }
        let socket: SocketAddr = texto
            .parse()
            .map_err(|_| ErrorDireccion::Ilegible(texto.to_string()))?;
        DireccionLocal::nueva(socket)
    }

    /// La dirección.
    pub fn socket(&self) -> SocketAddr {
        self.socket
    }
}

impl fmt::Display for DireccionLocal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.socket.fmt(f)
    }
}
