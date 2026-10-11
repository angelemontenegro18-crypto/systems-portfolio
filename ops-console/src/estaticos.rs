//! La interfaz, incrustada en el binario con `include_bytes!`: no se sirve nada desde el
//! disco, así que no hay ruta que recorrer ni archivo que cambiar por debajo.

/// Un archivo de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurso {
    /// La ruta con la que se pide.
    pub ruta: &'static str,
    /// El archivo de origen, dentro de `web/`.
    pub archivo: &'static str,
    /// Su `Content-Type`.
    pub tipo: &'static str,
    /// Su contenido.
    pub contenido: &'static [u8],
}

/// Todos los archivos de la interfaz.
pub static RECURSOS: [Recurso; 4] = [
    Recurso {
        ruta: "/",
        archivo: "index.html",
        tipo: "text/html; charset=utf-8",
        contenido: include_bytes!("../web/index.html"),
    },
    Recurso {
        ruta: "/app.css",
        archivo: "app.css",
        tipo: "text/css; charset=utf-8",
        contenido: include_bytes!("../web/app.css"),
    },
    Recurso {
        ruta: "/app.js",
        archivo: "app.js",
        tipo: "text/javascript; charset=utf-8",
        contenido: include_bytes!("../web/app.js"),
    },
    Recurso {
        ruta: "/icono.svg",
        archivo: "icono.svg",
        tipo: "image/svg+xml",
        contenido: include_bytes!("../web/icono.svg"),
    },
];

/// El recurso de `ruta`, si existe.
pub fn buscar(ruta: &str) -> Option<&'static Recurso> {
    RECURSOS.iter().find(|r| r.ruta == ruta)
}
