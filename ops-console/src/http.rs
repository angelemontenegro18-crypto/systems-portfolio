//! HTTP/1.1 mínimo y estricto, sobre `std`: una petición por conexión, límites explícitos y
//! nada interpretado a medias.
//!
//! - **Límites** ([`Limites`]): largo de la línea de petición (414), bytes y cantidad de
//!   cabeceras (431), tamaño del cuerpo (413) y tiempo total para recibir la petición (408).
//! - **Una petición por conexión.** Toda respuesta lleva `Connection: close`; no hay
//!   keep-alive ni cuerpos *chunked*.
//! - **Sin contrabando de peticiones.** `Transfer-Encoding` (cualquier valor) es 501;
//!   `Content-Length` repetido, con signo, espacios o comas es 400; un `POST` sin
//!   `Content-Length` es 411. Una cabecera con espacio antes de los dos puntos, una línea
//!   plegada o un fin de línea que no sea `\r\n` son 400.
//! - **Cabeceras de seguridad** ([`CABECERAS_DE_SEGURIDAD`]) en **toda** respuesta, también
//!   en los errores.

use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::json::Json;

/// Los límites del servidor. Los valores por defecto alcanzan de sobra para la consola.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    /// Bytes de la línea de petición, sin el `\r\n`. Más: 414.
    pub linea_max: usize,
    /// Bytes de todas las cabeceras juntas. Más: 431.
    pub cabeceras_max_bytes: usize,
    /// Cantidad de cabeceras. Más: 431.
    pub cabeceras_max: usize,
    /// Bytes del cuerpo. Más: 413.
    pub cuerpo_max: usize,
    /// Tiempo total para recibir la petición entera, desde que se acepta la conexión. Pasado
    /// ese tiempo: 408, aunque el cliente siga mandando bytes de a uno.
    pub tiempo_lectura: Duration,
    /// Tiempo máximo para escribir la respuesta.
    pub tiempo_escritura: Duration,
    /// Conexiones atendidas a la vez. La siguiente recibe 503 y se cierra.
    pub conexiones_max: usize,
}

impl Default for Limites {
    fn default() -> Limites {
        Limites {
            linea_max: 2048,
            cabeceras_max_bytes: 8 * 1024,
            cabeceras_max: 32,
            cuerpo_max: 16 * 1024,
            tiempo_lectura: Duration::from_secs(5),
            tiempo_escritura: Duration::from_secs(5),
            conexiones_max: 32,
        }
    }
}

/// Los métodos que la consola entiende.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metodo {
    /// Leer.
    Get,
    /// Cambiar estado.
    Post,
}

/// La cabeza de una petición ya validada: línea de petición y cabeceras.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cabeza {
    /// El método.
    pub metodo: Metodo,
    /// La ruta, sin la consulta.
    pub ruta: String,
    /// Lo que sigue a `?`, si hay.
    pub consulta: Option<String>,
    /// Las cabeceras, con el nombre en minúsculas y en el orden en que llegaron.
    pub cabeceras: Vec<(String, String)>,
    /// El `Content-Length`, si vino (uno solo y bien formado).
    pub longitud: Option<usize>,
}

impl Cabeza {
    /// El valor de la primera cabecera con ese nombre, sin distinguir mayúsculas.
    pub fn cabecera(&self, nombre: &str) -> Option<&str> {
        self.cabeceras
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nombre))
            .map(|(_, v)| v.as_str())
    }

    /// Cuántas veces vino una cabecera.
    pub fn veces(&self, nombre: &str) -> usize {
        self.cabeceras
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(nombre))
            .count()
    }
}

/// Una petición completa, con su cuerpo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peticion {
    /// La cabeza.
    pub cabeza: Cabeza,
    /// El cuerpo (vacío en un `GET`).
    pub cuerpo: Vec<u8>,
}

impl Peticion {
    /// El valor de un parámetro de la consulta (`?clave=valor`), sin decodificar.
    pub fn parametro(&self, clave: &str) -> Option<&str> {
        self.cabeza
            .consulta
            .as_deref()?
            .split('&')
            .filter_map(|par| par.split_once('='))
            .find(|(k, _)| *k == clave)
            .map(|(_, v)| v)
    }
}

/// Una petición rechazada antes de llegar a la aplicación: el estado HTTP y el motivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rechazo {
    /// Estado HTTP.
    pub estado: u16,
    /// Código estable, para la API.
    pub codigo: &'static str,
    /// Explicación para una persona.
    pub mensaje: String,
}

impl Rechazo {
    /// Un rechazo.
    pub fn nuevo(estado: u16, codigo: &'static str, mensaje: impl Into<String>) -> Rechazo {
        Rechazo {
            estado,
            codigo,
            mensaje: mensaje.into(),
        }
    }

    /// La respuesta de error que lo comunica.
    pub fn respuesta(&self) -> Respuesta {
        let mut r = Respuesta::error(self.estado, self.codigo, self.mensaje.clone());
        if self.estado == 405 {
            r = r.con_cabecera("Allow", "GET, POST");
        }
        r
    }
}

/// `tchar` de RFC 9110: lo que puede ir en un nombre de cabecera o de método.
fn es_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Analiza la cabeza de una petición: la línea de petición y las cabeceras, **sin** el
/// `\r\n\r\n` final. Es pura —no toca la red— para poder probarla byte a byte.
pub fn analizar_cabeza(cabeza: &[u8], limites: &Limites) -> Result<Cabeza, Rechazo> {
    let texto = std::str::from_utf8(cabeza)
        .map_err(|_| Rechazo::nuevo(400, "cabeza_ilegible", "la cabeza no es UTF-8"))?;
    let mut lineas = texto.split("\r\n");
    let linea = lineas.next().unwrap_or_default();
    if linea.len() > limites.linea_max {
        return Err(Rechazo::nuevo(
            414,
            "linea_demasiado_larga",
            format!("la línea de petición supera {} bytes", limites.linea_max),
        ));
    }
    if linea.contains(['\r', '\n']) {
        return Err(fin_de_linea_invalido());
    }

    let partes: Vec<&str> = linea.split(' ').collect();
    let [metodo, objetivo, version] = partes.as_slice() else {
        return Err(Rechazo::nuevo(
            400,
            "linea_malformada",
            "la línea de petición debe ser MÉTODO RUTA VERSIÓN, separados por un espacio",
        ));
    };
    if metodo.is_empty() || !metodo.bytes().all(es_tchar) {
        return Err(Rechazo::nuevo(
            400,
            "metodo_invalido",
            "el método no es válido",
        ));
    }
    match *version {
        "HTTP/1.1" | "HTTP/1.0" => {}
        v if v.starts_with("HTTP/") => {
            return Err(Rechazo::nuevo(
                505,
                "version_no_soportada",
                "solo se habla HTTP/1.1",
            ))
        }
        _ => {
            return Err(Rechazo::nuevo(
                400,
                "version_invalida",
                "versión HTTP ilegible",
            ))
        }
    }
    let metodo = match *metodo {
        "GET" => Metodo::Get,
        "POST" => Metodo::Post,
        otro => {
            return Err(Rechazo::nuevo(
                405,
                "metodo_no_permitido",
                format!("el método {otro} no está permitido: solo GET y POST"),
            ))
        }
    };
    if !objetivo.starts_with('/') {
        return Err(Rechazo::nuevo(
            400,
            "ruta_invalida",
            "la ruta debe empezar con «/» (no se aceptan URL absolutas)",
        ));
    }
    if !objetivo.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(Rechazo::nuevo(
            400,
            "ruta_invalida",
            "la ruta tiene caracteres no válidos",
        ));
    }
    let (ruta, consulta) = match objetivo.split_once('?') {
        Some((r, c)) => (r.to_string(), Some(c.to_string())),
        None => (objetivo.to_string(), None),
    };

    let mut cabeceras = Vec::new();
    let mut bytes_cabeceras = 0usize;
    for linea in lineas {
        bytes_cabeceras += linea.len() + 2;
        if bytes_cabeceras > limites.cabeceras_max_bytes {
            return Err(cabeceras_demasiado_grandes(limites));
        }
        if linea.contains(['\r', '\n']) {
            return Err(fin_de_linea_invalido());
        }
        if linea.starts_with([' ', '\t']) {
            return Err(Rechazo::nuevo(
                400,
                "cabecera_plegada",
                "las cabeceras plegadas en varias líneas no se aceptan",
            ));
        }
        let Some((nombre, valor)) = linea.split_once(':') else {
            return Err(Rechazo::nuevo(
                400,
                "cabecera_malformada",
                "falta «:» en una cabecera",
            ));
        };
        if nombre.is_empty() || !nombre.bytes().all(es_tchar) {
            return Err(Rechazo::nuevo(
                400,
                "cabecera_malformada",
                "nombre de cabecera inválido (¿espacio antes de los dos puntos?)",
            ));
        }
        let valor = valor.trim_matches([' ', '\t']);
        if valor.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(Rechazo::nuevo(
                400,
                "cabecera_malformada",
                "el valor de una cabecera tiene caracteres de control",
            ));
        }
        cabeceras.push((nombre.to_ascii_lowercase(), valor.to_string()));
        if cabeceras.len() > limites.cabeceras_max {
            return Err(Rechazo::nuevo(
                431,
                "demasiadas_cabeceras",
                format!("más de {} cabeceras", limites.cabeceras_max),
            ));
        }
    }

    let mut cabeza = Cabeza {
        metodo,
        ruta,
        consulta,
        cabeceras,
        longitud: None,
    };

    // Contrabando de peticiones: lo que dos servidores podrían leer distinto, no se lee.
    if cabeza.veces("transfer-encoding") > 0 {
        return Err(Rechazo::nuevo(
            501,
            "transfer_encoding_no_soportado",
            "Transfer-Encoding no está soportado: usa Content-Length",
        ));
    }
    match cabeza.veces("content-length") {
        0 => {}
        1 => {
            let valor = cabeza.cabecera("content-length").unwrap_or_default();
            if valor.is_empty() || valor.len() > 18 || !valor.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Rechazo::nuevo(
                    400,
                    "content_length_invalido",
                    "Content-Length debe ser un número sin signo, espacios ni comas",
                ));
            }
            cabeza.longitud = valor.parse().ok();
        }
        _ => {
            return Err(Rechazo::nuevo(
                400,
                "content_length_repetido",
                "Content-Length vino más de una vez",
            ))
        }
    }
    match cabeza.veces("host") {
        1 => {}
        0 => return Err(Rechazo::nuevo(400, "sin_host", "falta la cabecera Host")),
        _ => {
            return Err(Rechazo::nuevo(
                400,
                "host_repetido",
                "Host vino más de una vez",
            ))
        }
    }
    if cabeza.cabecera("expect").is_some() {
        return Err(Rechazo::nuevo(
            417,
            "expect_no_soportado",
            "Expect no está soportado: manda el cuerpo directamente",
        ));
    }
    Ok(cabeza)
}

fn fin_de_linea_invalido() -> Rechazo {
    Rechazo::nuevo(
        400,
        "fin_de_linea_invalido",
        "las líneas deben terminar en \\r\\n",
    )
}

fn cabeceras_demasiado_grandes(limites: &Limites) -> Rechazo {
    Rechazo::nuevo(
        431,
        "cabeceras_demasiado_grandes",
        format!(
            "las cabeceras superan {} bytes",
            limites.cabeceras_max_bytes
        ),
    )
}

fn plazo_vencido() -> Rechazo {
    Rechazo::nuevo(
        408,
        "tiempo_agotado",
        "la petición no llegó completa a tiempo",
    )
}

/// Lee de la conexión con lo que queda de plazo como tiempo máximo. `Ok(0)` es fin de flujo.
fn leer_con_plazo(flujo: &mut TcpStream, buf: &mut [u8], plazo: Instant) -> Result<usize, Rechazo> {
    loop {
        let resta = plazo.saturating_duration_since(Instant::now());
        if resta.is_zero() {
            return Err(plazo_vencido());
        }
        flujo
            .set_read_timeout(Some(resta))
            .map_err(|_| Rechazo::nuevo(500, "fallo_interno", "no se pudo fijar el plazo"))?;
        match flujo.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(plazo_vencido())
            }
            Err(_) => {
                return Err(Rechazo::nuevo(
                    400,
                    "conexion_rota",
                    "la conexión se cortó a mitad de la petición",
                ))
            }
        }
    }
}

/// Lee la cabeza de la petición, hasta `\r\n\r\n`, vigilando los límites mientras llegan los
/// bytes: una línea o unas cabeceras demasiado largas se rechazan sin esperar al final.
/// Devuelve la cabeza (sin el `\r\n\r\n`) y lo que ya llegó del cuerpo.
pub fn leer_cabeza(
    flujo: &mut TcpStream,
    limites: &Limites,
    plazo: Instant,
) -> Result<(Vec<u8>, Vec<u8>), Rechazo> {
    let tope = limites.linea_max + 2 + limites.cabeceras_max_bytes + 2;
    let mut acumulado: Vec<u8> = Vec::with_capacity(1024);
    let mut trozo = [0u8; 2048];
    loop {
        if let Some(fin) = buscar(&acumulado, b"\r\n\r\n") {
            let sobrante = acumulado.split_off(fin + 4);
            acumulado.truncate(fin);
            return Ok((acumulado, sobrante));
        }
        // Una cabeza que termina con saltos de línea sin `\r` no se espera hasta el plazo:
        // se rechaza en cuanto se la ve.
        if buscar(&acumulado, b"\n\n").is_some() {
            return Err(fin_de_linea_invalido());
        }
        match buscar(&acumulado, b"\r\n") {
            None if acumulado.len() > limites.linea_max => {
                return Err(Rechazo::nuevo(
                    414,
                    "linea_demasiado_larga",
                    format!("la línea de petición supera {} bytes", limites.linea_max),
                ))
            }
            Some(fin_linea) if acumulado.len() - fin_linea > limites.cabeceras_max_bytes + 4 => {
                return Err(cabeceras_demasiado_grandes(limites))
            }
            _ if acumulado.len() > tope => return Err(cabeceras_demasiado_grandes(limites)),
            _ => {}
        }
        let n = leer_con_plazo(flujo, &mut trozo, plazo)?;
        if n == 0 {
            return Err(Rechazo::nuevo(
                400,
                "peticion_incompleta",
                "la conexión se cerró antes de terminar la cabeza",
            ));
        }
        acumulado.extend_from_slice(&trozo[..n]);
    }
}

/// Lee exactamente `longitud` bytes de cuerpo (los primeros pueden haber llegado con la
/// cabeza). Nunca lee más allá: lo que sobre en la conexión se descarta al cerrarla.
pub fn leer_cuerpo(
    flujo: &mut TcpStream,
    longitud: usize,
    mut ya_llegado: Vec<u8>,
    plazo: Instant,
) -> Result<Vec<u8>, Rechazo> {
    ya_llegado.truncate(longitud);
    let mut trozo = [0u8; 4096];
    while ya_llegado.len() < longitud {
        let falta = (longitud - ya_llegado.len()).min(trozo.len());
        let n = leer_con_plazo(flujo, &mut trozo[..falta], plazo)?;
        if n == 0 {
            return Err(Rechazo::nuevo(
                400,
                "cuerpo_incompleto",
                "llegó menos cuerpo que el que anunció Content-Length",
            ));
        }
        ya_llegado.extend_from_slice(&trozo[..n]);
    }
    Ok(ya_llegado)
}

fn buscar(pajar: &[u8], aguja: &[u8]) -> Option<usize> {
    pajar.windows(aguja.len()).position(|v| v == aguja)
}

/// Cabeceras que lleva **toda** respuesta.
///
/// La política de contenido no permite nada que no venga del propio servidor —ni scripts ni
/// estilos en línea, ni recursos de otros orígenes—, prohíbe que otra página incruste la
/// consola en un marco y que se envíen formularios.
pub const CABECERAS_DE_SEGURIDAD: [(&str, &str); 8] = [
    (
        "Content-Security-Policy",
        "default-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
    ),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Frame-Options", "DENY"),
    ("Cache-Control", "no-store"),
    ("Cross-Origin-Opener-Policy", "same-origin"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    (
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=(), payment=()",
    ),
];

/// Una respuesta lista para enviar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Respuesta {
    /// Estado HTTP.
    pub estado: u16,
    /// `Content-Type`.
    pub tipo: &'static str,
    /// El cuerpo.
    pub cuerpo: Vec<u8>,
    /// Cabeceras propias de esta respuesta (`Allow`, `Retry-After`…).
    pub extra: Vec<(&'static str, String)>,
}

const TIPO_JSON: &str = "application/json; charset=utf-8";

impl Respuesta {
    /// `200` con `{"ok": true, "datos": …}`.
    pub fn ok(datos: Json) -> Respuesta {
        Respuesta {
            estado: 200,
            tipo: TIPO_JSON,
            cuerpo: Json::objeto([("ok", Json::Bool(true)), ("datos", datos)])
                .a_texto()
                .into_bytes(),
            extra: Vec::new(),
        }
    }

    /// Un error con `{"ok": false, "error": {"estado", "codigo", "mensaje"}}`: la misma forma
    /// para todos.
    pub fn error(estado: u16, codigo: &str, mensaje: impl Into<String>) -> Respuesta {
        let cuerpo = Json::objeto([
            ("ok", Json::Bool(false)),
            (
                "error",
                Json::objeto([
                    ("estado", Json::from(u32::from(estado))),
                    ("codigo", Json::from(codigo)),
                    ("mensaje", Json::from(mensaje.into())),
                ]),
            ),
        ]);
        Respuesta {
            estado,
            tipo: TIPO_JSON,
            cuerpo: cuerpo.a_texto().into_bytes(),
            extra: Vec::new(),
        }
    }

    /// Un recurso estático incrustado en el binario.
    pub fn estatico(tipo: &'static str, cuerpo: &'static [u8]) -> Respuesta {
        Respuesta {
            estado: 200,
            tipo,
            cuerpo: cuerpo.to_vec(),
            extra: Vec::new(),
        }
    }

    /// La misma respuesta con una cabecera más.
    pub fn con_cabecera(mut self, nombre: &'static str, valor: impl Into<String>) -> Respuesta {
        self.extra.push((nombre, valor.into()));
        self
    }

    /// Los bytes que viajan por la conexión.
    pub fn serializar(&self) -> Vec<u8> {
        let mut cabeza = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.estado,
            razon(self.estado),
            self.tipo,
            self.cuerpo.len()
        );
        for (nombre, valor) in CABECERAS_DE_SEGURIDAD {
            cabeza.push_str(&format!("{nombre}: {valor}\r\n"));
        }
        for (nombre, valor) in &self.extra {
            cabeza.push_str(&format!("{nombre}: {valor}\r\n"));
        }
        cabeza.push_str("\r\n");
        let mut bytes = cabeza.into_bytes();
        bytes.extend_from_slice(&self.cuerpo);
        bytes
    }
}

/// La frase de cada estado que usa la consola.
pub fn razon(estado: u16) -> &'static str {
    match estado {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        417 => "Expectation Failed",
        421 => "Misdirected Request",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Error",
    }
}
