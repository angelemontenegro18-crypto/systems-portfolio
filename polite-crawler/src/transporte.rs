//! El transporte: un `GET` y nada más.
//!
//! El rastreador no conoce HTTP; habla con un [`Transporte`]. Eso permite
//! probarlo con un transporte falso y un reloj controlado, y deja la red real
//! en un solo lugar, [`TransporteHttp`].
//!
//! El transporte **no sigue redirecciones**: una redirección puede llevar a
//! otro host, con su propio `robots.txt` y su propio límite. El rastreador la
//! devuelve, y quien lo usa decide pedir la nueva URL, que pasa por todas las
//! reglas como cualquier otra.

use std::io::Read;
use std::time::Duration;

use url::Url;

/// Lo que devolvió el servidor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Respuesta {
    /// Código de estado HTTP.
    pub estado: u16,
    /// Cuerpo, recortado al máximo configurado.
    pub cuerpo: Vec<u8>,
    /// `true` si el cuerpo era más largo y se recortó.
    pub recortado: bool,
    /// Encabezado `Retry-After`, tal cual.
    pub retry_after: Option<String>,
    /// Encabezado `Location`, tal cual.
    pub location: Option<String>,
}

/// Un `GET` con la identidad dada. Solo `GET`: no hay otro método.
pub trait Transporte {
    /// Pide `url` presentándose como `user_agent`. Un error es una falla de red,
    /// no un estado HTTP: los 4xx y 5xx llegan como [`Respuesta`].
    fn get(&self, url: &Url, user_agent: &str) -> Result<Respuesta, String>;
}

/// Transporte HTTP/HTTPS real.
pub struct TransporteHttp {
    agente: ureq::Agent,
    max_bytes: usize,
}

impl TransporteHttp {
    /// `timeout` acota la conexión y la lectura; `max_bytes`, el cuerpo.
    pub fn nuevo(timeout: Duration, max_bytes: usize) -> Self {
        let agente = ureq::AgentBuilder::new().redirects(0).timeout(timeout).build();
        Self { agente, max_bytes }
    }
}

impl Transporte for TransporteHttp {
    fn get(&self, url: &Url, user_agent: &str) -> Result<Respuesta, String> {
        let respuesta = match self.agente.get(url.as_str()).set("User-Agent", user_agent).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(t.to_string()),
        };
        let estado = respuesta.status();
        let retry_after = respuesta.header("Retry-After").map(str::to_string);
        let location = respuesta.header("Location").map(str::to_string);

        let mut cuerpo = Vec::new();
        let tope = u64::try_from(self.max_bytes).unwrap_or(u64::MAX).saturating_add(1);
        respuesta.into_reader().take(tope).read_to_end(&mut cuerpo).map_err(|e| e.to_string())?;
        let recortado = cuerpo.len() > self.max_bytes;
        cuerpo.truncate(self.max_bytes);

        Ok(Respuesta { estado, cuerpo, recortado, retry_after, location })
    }
}
