//! JSON mínimo, sin dependencias: un escritor para las respuestas y un analizador **estricto**
//! para los cuerpos de las peticiones.
//!
//! El analizador acepta solo lo que la API necesita y rechaza lo ambiguo: claves duplicadas,
//! números con decimales o exponente, anidamiento de más de [`PROFUNDIDAD_MAX`] niveles y
//! cualquier cosa después del valor. Un cuerpo que no se entiende entero no se interpreta a
//! medias.

use std::fmt::Write as _;

/// Niveles de anidamiento que acepta [`analizar`].
pub const PROFUNDIDAD_MAX: usize = 8;

/// Un valor JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    /// `null`.
    Nulo,
    /// `true` o `false`.
    Bool(bool),
    /// Un entero. La API no usa números con decimales.
    Entero(i64),
    /// Una cadena.
    Texto(String),
    /// Una lista.
    Lista(Vec<Json>),
    /// Un objeto, con sus claves en orden.
    Objeto(Vec<(String, Json)>),
}

impl Json {
    /// Un objeto a partir de pares clave–valor.
    pub fn objeto<'a>(pares: impl IntoIterator<Item = (&'a str, Json)>) -> Json {
        Json::Objeto(
            pares
                .into_iter()
                .map(|(clave, valor)| (clave.to_string(), valor))
                .collect(),
        )
    }

    /// El valor de `clave`, si es un objeto y la tiene.
    pub fn campo(&self, clave: &str) -> Option<&Json> {
        match self {
            Json::Objeto(pares) => pares.iter().find(|(k, _)| k == clave).map(|(_, v)| v),
            _ => None,
        }
    }

    /// El entero, si lo es.
    pub fn como_entero(&self) -> Option<i64> {
        match self {
            Json::Entero(n) => Some(*n),
            _ => None,
        }
    }

    /// Las claves, si es un objeto.
    pub fn claves(&self) -> Vec<&str> {
        match self {
            Json::Objeto(pares) => pares.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    /// El texto JSON, compacto.
    pub fn a_texto(&self) -> String {
        let mut salida = String::new();
        escribir(self, &mut salida);
        salida
    }
}

impl From<bool> for Json {
    fn from(valor: bool) -> Json {
        Json::Bool(valor)
    }
}

impl From<i64> for Json {
    fn from(valor: i64) -> Json {
        Json::Entero(valor)
    }
}

impl From<u32> for Json {
    fn from(valor: u32) -> Json {
        Json::Entero(i64::from(valor))
    }
}

impl From<u64> for Json {
    fn from(valor: u64) -> Json {
        Json::Entero(i64::try_from(valor).unwrap_or(i64::MAX))
    }
}

impl From<usize> for Json {
    fn from(valor: usize) -> Json {
        Json::Entero(i64::try_from(valor).unwrap_or(i64::MAX))
    }
}

impl From<&str> for Json {
    fn from(valor: &str) -> Json {
        Json::Texto(valor.to_string())
    }
}

impl From<String> for Json {
    fn from(valor: String) -> Json {
        Json::Texto(valor)
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(valor: Option<T>) -> Json {
        valor.map_or(Json::Nulo, Into::into)
    }
}

impl From<Vec<Json>> for Json {
    fn from(valor: Vec<Json>) -> Json {
        Json::Lista(valor)
    }
}

fn escribir(valor: &Json, salida: &mut String) {
    match valor {
        Json::Nulo => salida.push_str("null"),
        Json::Bool(true) => salida.push_str("true"),
        Json::Bool(false) => salida.push_str("false"),
        Json::Entero(n) => {
            let _ = write!(salida, "{n}");
        }
        Json::Texto(texto) => escribir_texto(texto, salida),
        Json::Lista(elementos) => {
            salida.push('[');
            for (i, elemento) in elementos.iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                escribir(elemento, salida);
            }
            salida.push(']');
        }
        Json::Objeto(pares) => {
            salida.push('{');
            for (i, (clave, valor)) in pares.iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                escribir_texto(clave, salida);
                salida.push(':');
                escribir(valor, salida);
            }
            salida.push('}');
        }
    }
}

/// Una cadena JSON. Además de lo obligatorio (comillas, barra invertida y controles), escapa
/// `<`, `>`, `&` y los separadores U+2028/U+2029: el texto queda inerte aunque alguien lo
/// pegue dentro de un HTML.
fn escribir_texto(texto: &str, salida: &mut String) {
    salida.push('"');
    for c in texto.chars() {
        match c {
            '"' => salida.push_str("\\\""),
            '\\' => salida.push_str("\\\\"),
            '\n' => salida.push_str("\\n"),
            '\r' => salida.push_str("\\r"),
            '\t' => salida.push_str("\\t"),
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(salida, "\\u{:04x}", u32::from(c));
            }
            c if u32::from(c) < 0x20 || c == '\u{7f}' => {
                let _ = write!(salida, "\\u{:04x}", u32::from(c));
            }
            c => salida.push(c),
        }
    }
    salida.push('"');
}

/// Por qué un texto no es el JSON que se esperaba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorJson {
    /// Byte donde se detectó el problema.
    pub posicion: usize,
    /// Qué pasó.
    pub motivo: &'static str,
}

impl std::fmt::Display for ErrorJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JSON inválido en el byte {}: {}",
            self.posicion, self.motivo
        )
    }
}

impl std::error::Error for ErrorJson {}

/// Analiza `texto` como un único valor JSON, de forma estricta.
pub fn analizar(texto: &str) -> Result<Json, ErrorJson> {
    let mut a = Analizador {
        bytes: texto.as_bytes(),
        pos: 0,
    };
    a.espacios();
    let valor = a.valor(0)?;
    a.espacios();
    if a.pos != a.bytes.len() {
        return Err(a.error("sobra texto después del valor"));
    }
    Ok(valor)
}

struct Analizador<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Analizador<'_> {
    fn error(&self, motivo: &'static str) -> ErrorJson {
        ErrorJson {
            posicion: self.pos,
            motivo,
        }
    }

    fn ver(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn espacios(&mut self) {
        while matches!(self.ver(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn literal(&mut self, palabra: &[u8], valor: Json) -> Result<Json, ErrorJson> {
        if self.bytes[self.pos..].starts_with(palabra) {
            self.pos += palabra.len();
            Ok(valor)
        } else {
            Err(self.error("literal desconocido"))
        }
    }

    fn valor(&mut self, profundidad: usize) -> Result<Json, ErrorJson> {
        if profundidad > PROFUNDIDAD_MAX {
            return Err(self.error("anidamiento excesivo"));
        }
        match self.ver() {
            Some(b'{') => self.objeto(profundidad),
            Some(b'[') => self.lista(profundidad),
            Some(b'"') => self.texto().map(Json::Texto),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'n') => self.literal(b"null", Json::Nulo),
            Some(b'-' | b'0'..=b'9') => self.entero(),
            Some(_) => Err(self.error("se esperaba un valor")),
            None => Err(self.error("el texto terminó antes del valor")),
        }
    }

    fn objeto(&mut self, profundidad: usize) -> Result<Json, ErrorJson> {
        self.pos += 1;
        let mut pares: Vec<(String, Json)> = Vec::new();
        self.espacios();
        if self.ver() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Objeto(pares));
        }
        loop {
            self.espacios();
            if self.ver() != Some(b'"') {
                return Err(self.error("se esperaba una clave entre comillas"));
            }
            let clave = self.texto()?;
            if pares.iter().any(|(k, _)| *k == clave) {
                return Err(self.error("clave repetida"));
            }
            self.espacios();
            if self.ver() != Some(b':') {
                return Err(self.error("se esperaban dos puntos"));
            }
            self.pos += 1;
            self.espacios();
            let valor = self.valor(profundidad + 1)?;
            pares.push((clave, valor));
            self.espacios();
            match self.ver() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Objeto(pares));
                }
                _ => return Err(self.error("se esperaba una coma o una llave de cierre")),
            }
        }
    }

    fn lista(&mut self, profundidad: usize) -> Result<Json, ErrorJson> {
        self.pos += 1;
        let mut elementos = Vec::new();
        self.espacios();
        if self.ver() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Lista(elementos));
        }
        loop {
            self.espacios();
            elementos.push(self.valor(profundidad + 1)?);
            self.espacios();
            match self.ver() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Lista(elementos));
                }
                _ => return Err(self.error("se esperaba una coma o un corchete de cierre")),
            }
        }
    }

    fn entero(&mut self) -> Result<Json, ErrorJson> {
        let inicio = self.pos;
        if self.ver() == Some(b'-') {
            self.pos += 1;
        }
        let digitos = self.pos;
        while matches!(self.ver(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == digitos {
            return Err(self.error("se esperaban dígitos"));
        }
        if self.bytes[digitos] == b'0' && self.pos - digitos > 1 {
            return Err(self.error("ceros a la izquierda"));
        }
        if matches!(self.ver(), Some(b'.' | b'e' | b'E')) {
            return Err(self.error("solo se aceptan números enteros"));
        }
        let texto = std::str::from_utf8(&self.bytes[inicio..self.pos])
            .map_err(|_| self.error("número ilegible"))?;
        texto
            .parse::<i64>()
            .map(Json::Entero)
            .map_err(|_| self.error("número fuera de rango"))
    }

    fn texto(&mut self) -> Result<String, ErrorJson> {
        self.pos += 1;
        let mut salida = String::new();
        loop {
            let resto = &self.bytes[self.pos..];
            let Some(desde) = resto
                .iter()
                .position(|&b| b == b'"' || b == b'\\' || b < 0x20)
            else {
                return Err(self.error("cadena sin cerrar"));
            };
            let tramo =
                std::str::from_utf8(&resto[..desde]).map_err(|_| self.error("UTF-8 inválido"))?;
            salida.push_str(tramo);
            self.pos += desde;
            match self.bytes[self.pos] {
                b'"' => {
                    self.pos += 1;
                    return Ok(salida);
                }
                b'\\' => {
                    self.pos += 1;
                    let escape = self.ver().ok_or_else(|| self.error("escape incompleto"))?;
                    self.pos += 1;
                    match escape {
                        b'"' => salida.push('"'),
                        b'\\' => salida.push('\\'),
                        b'/' => salida.push('/'),
                        b'b' => salida.push('\u{8}'),
                        b'f' => salida.push('\u{c}'),
                        b'n' => salida.push('\n'),
                        b'r' => salida.push('\r'),
                        b't' => salida.push('\t'),
                        b'u' => {
                            let c = self.escape_unicode()?;
                            salida.push(c);
                        }
                        _ => return Err(self.error("escape desconocido")),
                    }
                }
                _ => return Err(self.error("carácter de control sin escapar")),
            }
        }
    }

    fn cuatro_hex(&mut self) -> Result<u32, ErrorJson> {
        let tramo = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("escape \\u incompleto"))?;
        // `from_str_radix` aceptaría un signo (`\u+041`): se exigen cuatro dígitos hexadecimales.
        if !tramo.iter().all(u8::is_ascii_hexdigit) {
            return Err(self.error("escape \\u inválido"));
        }
        let texto = std::str::from_utf8(tramo).map_err(|_| self.error("escape \\u inválido"))?;
        let valor =
            u32::from_str_radix(texto, 16).map_err(|_| self.error("escape \\u inválido"))?;
        self.pos += 4;
        Ok(valor)
    }

    fn escape_unicode(&mut self) -> Result<char, ErrorJson> {
        let alto = self.cuatro_hex()?;
        let punto = if (0xD800..0xDC00).contains(&alto) {
            if self.bytes[self.pos..].starts_with(b"\\u") {
                self.pos += 2;
                let bajo = self.cuatro_hex()?;
                if !(0xDC00..0xE000).contains(&bajo) {
                    return Err(self.error("par sustituto inválido"));
                }
                0x10000 + ((alto - 0xD800) << 10) + (bajo - 0xDC00)
            } else {
                return Err(self.error("sustituto alto sin pareja"));
            }
        } else if (0xDC00..0xE000).contains(&alto) {
            return Err(self.error("sustituto bajo suelto"));
        } else {
            alto
        };
        char::from_u32(punto).ok_or_else(|| self.error("punto de código inválido"))
    }
}
