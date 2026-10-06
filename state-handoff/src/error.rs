//! El error común del traspaso.

use core::fmt;

/// Lo que puede salir mal al sellar, verificar o preparar un paquete. Ninguna función de la
/// biblioteca entra en pánico: todo camino de error termina aquí.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Faltan bytes: menos que una cabecera, o menos que lo que la cabecera anuncia.
    Truncado,
    /// Sobran bytes después del contenido que la cabecera anuncia.
    Sobrante,
    /// El búfer de salida no alcanza.
    SalidaCorta,
    /// El contenido no cabe: en el área de preparación, o en el campo de largo.
    NoCabe,
    /// La firma no es la de este protocolo.
    FirmaAjena,
    /// Una versión del formato que este código no entiende.
    VersionDesconocida(u16),
    /// El CRC de la cabecera no coincide.
    CabeceraCorrupta,
    /// El CRC del contenido no coincide.
    ContenidoCorrupto,
    /// El paquete es para otro nodo.
    DestinoAjeno(u16),
    /// La época no es mayor que la última confirmada: un paquete repetido o viejo.
    EpocaVieja {
        /// La última época confirmada.
        ultima: u64,
        /// La que traía el paquete.
        recibida: u64,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncado => f.write_str("faltan bytes"),
            Error::Sobrante => f.write_str("sobran bytes después del contenido"),
            Error::SalidaCorta => f.write_str("el búfer de salida es demasiado corto"),
            Error::NoCabe => f.write_str("el contenido no cabe"),
            Error::FirmaAjena => f.write_str("la firma no es la de este protocolo"),
            Error::VersionDesconocida(v) => write!(f, "versión de formato desconocida: {v}"),
            Error::CabeceraCorrupta => f.write_str("el CRC de la cabecera no coincide"),
            Error::ContenidoCorrupto => f.write_str("el CRC del contenido no coincide"),
            Error::DestinoAjeno(d) => write!(f, "el paquete es para el nodo {d}"),
            Error::EpocaVieja { ultima, recibida } => write!(
                f,
                "época {recibida} no posterior a la última confirmada ({ultima})"
            ),
        }
    }
}
