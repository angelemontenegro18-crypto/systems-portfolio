//! La cola de un archivo de registro, con **tope de bytes** y **redacción** de lo sensible.
//!
//! - [`leer_cola`] lee como mucho `tope` bytes **desde el final** (más [`CONTEXTO`]): un
//!   archivo enorme no se lee entero.
//! - [`redactar`] reemplaza por [`MARCA`] los correos, los tokens (`token=…`, `Bearer …`,
//!   claves `sk-…`), los identificadores de cuenta (`CLI-123456`), lo que cuelga de los
//!   directorios sensibles (`/etc/secretos/…`, `C:\Secretos\…`) y las credenciales de una URL
//!   (`https://usuario:clave@…`, `…/cuentas/<id>/…`). No distingue mayúsculas, sigue a un
//!   valor que quedó en la línea siguiente a su clave, y es **idempotente**: redactar dos
//!   veces da lo mismo que una. Lo demás queda tal cual.
//!
//! El texto que sale hacia la API es un [`Redactado`], y un `Redactado` solo lo crea
//! [`redactar`]: no hay forma de devolver texto crudo donde se espera uno.
//!
//! ```compile_fail,E0451
//! use ops_console::registros::Redactado;
//!
//! // error[E0451]: el campo es privado; solo `redactar` crea un `Redactado`.
//! let crudo = Redactado { texto: String::from("token=abc123") };
//! ```
//!
//! ```compile_fail,E0308
//! use ops_console::registros::Redactado;
//!
//! fn para_la_api() -> Redactado {
//!     // error[E0308]: se esperaba un `Redactado`, no un `String`.
//!     String::from("token=abc123")
//! }
//! ```

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Lo que reemplaza a cada dato sensible.
pub const MARCA: &str = "[REDACTADO]";

/// Distancia máxima, en bytes, entre el comienzo de una clave (`token`, `Bearer`…) y el de su
/// valor. Más lejos, ya no se consideran ligados.
pub const ALCANCE: usize = 256;

/// Bytes que [`leer_cola`] lee **antes** de la ventana pedida, solo como contexto. Como es
/// mayor que [`ALCANCE`], una clave que quedó justo antes del corte sigue alcanzando a su
/// valor: cada línea que se muestra sale redactada igual que si se hubiera leído el archivo
/// entero.
pub const CONTEXTO: u64 = 1024;

/// Largo máximo de una palabra para tomarla como clave.
const CLAVE_MAX: usize = 64;

/// Texto que ya pasó por [`redactar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redactado {
    texto: String,
}

impl Redactado {
    /// El texto redactado.
    pub fn como_str(&self) -> &str {
        &self.texto
    }

    /// El texto partido en líneas (sin `\n` ni `\r` final), cada una todavía un `Redactado`.
    /// El salto de línea final no abre una línea vacía.
    pub fn lineas(&self) -> Vec<Redactado> {
        let cuerpo = self.texto.strip_suffix('\n').unwrap_or(&self.texto);
        if cuerpo.is_empty() {
            return Vec::new();
        }
        cuerpo
            .split('\n')
            .map(|l| Redactado {
                texto: l.strip_suffix('\r').unwrap_or(l).to_string(),
            })
            .collect()
    }
}

impl fmt::Display for Redactado {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.texto)
    }
}

/// Redacta todo lo sensible de `texto`.
///
/// Ningún paso borra ni agrega saltos de línea: el texto redactado tiene las mismas líneas
/// que el original.
pub fn redactar(texto: &str) -> Redactado {
    let pasos: [fn(&str) -> String; 8] = [
        credenciales_en_url,
        cuentas_en_ruta,
        rutas_sensibles,
        portador,
        valores_con_clave,
        claves_con_prefijo,
        identificadores_de_cuenta,
        correos,
    ];
    let mut actual = texto.to_string();
    for paso in pasos {
        actual = paso(&actual);
    }
    Redactado { texto: actual }
}

/// Reemplaza por la marca los tramos `[inicio, fin)`, que llegan en orden y sin solaparse.
/// Todos empiezan y terminan junto a un byte ASCII o en un extremo, así que caen en límites
/// de carácter.
fn reemplazar(texto: &str, tramos: &[(usize, usize)]) -> String {
    let mut salida = String::with_capacity(texto.len());
    let mut desde = 0;
    for &(inicio, fin) in tramos {
        salida.push_str(&texto[desde..inicio]);
        salida.push_str(MARCA);
        desde = fin;
    }
    salida.push_str(&texto[desde..]);
    salida
}

/// Agrega un tramo (los tramos llegan ordenados por su inicio). Uno vacío o que ya es la
/// marca no se agrega; uno que se solapa con el anterior lo extiende.
fn agregar(tramos: &mut Vec<(usize, usize)>, texto: &str, inicio: usize, fin: usize) {
    if fin <= inicio || &texto[inicio..fin] == MARCA {
        return;
    }
    match tramos.last_mut() {
        Some(ultimo) if inicio < ultimo.1 => ultimo.1 = ultimo.1.max(fin),
        _ => tramos.push((inicio, fin)),
    }
}

/// Las posiciones donde empieza `aguja` en `pajar`.
fn apariciones(pajar: &[u8], aguja: &[u8]) -> Vec<usize> {
    if aguja.is_empty() || pajar.len() < aguja.len() {
        return Vec::new();
    }
    (0..=pajar.len() - aguja.len())
        .filter(|&i| &pajar[i..i + aguja.len()] == aguja)
        .collect()
}

/// Si en `i` empieza una palabra: al principio, o después de algo que no es letra ni dígito.
fn inicio_de_palabra(b: &[u8], i: usize) -> bool {
    i == 0 || !b[i - 1].is_ascii_alphanumeric()
}

/// El primer byte desde `desde` para el que `corta` es cierto, o el final.
fn fin_de_tramo(b: &[u8], desde: usize, corta: impl Fn(u8) -> bool) -> usize {
    b[desde..]
        .iter()
        .position(|&c| corta(c))
        .map_or(b.len(), |p| desde + p)
}

/// Retrocede desde `hasta` mientras `sigue` sea cierto; devuelve dónde se detuvo.
fn fin_de_tramo_atras(b: &[u8], hasta: usize, sigue: impl Fn(u8) -> bool) -> usize {
    b[..hasta]
        .iter()
        .rposition(|&c| !sigue(c))
        .map_or(0, |q| q + 1)
}

/// Como [`fin_de_tramo`], pero si el valor empieza con la marca la salta entera: así un valor
/// ya redactado se reconoce aunque la marca tenga un carácter que corta.
fn fin_de_valor(b: &[u8], desde: usize, corta: impl Fn(u8) -> bool) -> usize {
    let marca = MARCA.as_bytes();
    let ya_redactado = b
        .get(desde..desde + marca.len())
        .is_some_and(|v| v.eq_ignore_ascii_case(marca));
    let desde = if ya_redactado {
        desde + marca.len()
    } else {
        desde
    };
    fin_de_tramo(b, desde, corta)
}

fn saltar_espacios(b: &[u8], desde: usize) -> usize {
    fin_de_tramo(b, desde, |c| !c.is_ascii_whitespace())
}

/// Lo que termina un valor suelto (sin comillas).
fn es_delimitador(c: u8) -> bool {
    c.is_ascii_whitespace() || b"\"'<>`".contains(&c)
}

/// `esquema://usuario:clave@host` → `esquema://[REDACTADO]@host`.
fn credenciales_en_url(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut tramos = Vec::new();
    for p in apariciones(b, b"://") {
        let inicio = p + 3;
        let fin = fin_de_tramo(b, inicio, |c| es_delimitador(c) || b"/?#".contains(&c));
        if let Some(arroba) = b[inicio..fin].iter().rposition(|&c| c == b'@') {
            agregar(&mut tramos, texto, inicio, inicio + arroba);
        }
    }
    reemplazar(texto, &tramos)
}

/// `…/cuentas/<id>/…` → `…/cuentas/[REDACTADO]/…`.
fn cuentas_en_ruta(texto: &str) -> String {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let mut tramos = Vec::new();
    for p in apariciones(b, b"/cuentas/") {
        let inicio = p + "/cuentas/".len();
        let fin = fin_de_valor(b, inicio, |c| es_delimitador(c) || b"/?#".contains(&c));
        agregar(&mut tramos, texto, inicio, fin);
    }
    reemplazar(texto, &tramos)
}

/// Lo que cuelga de un directorio `secretos` en la raíz de configuración —`/etc/secretos/…`—
/// o de una unidad de Windows —`C:\Secretos\…`, también con `/` o con barras dobles—.
fn rutas_sensibles(texto: &str) -> String {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let es_barra = |c: u8| c == b'/' || c == b'\\';
    let mut tramos = Vec::new();
    for p in apariciones(b, b"secretos") {
        let raiz = fin_de_tramo_atras(b, p, es_barra);
        let en_etc = raiz >= 4 && &b[raiz - 4..raiz] == b"/etc";
        let en_unidad = raiz >= 2 && b[raiz - 1] == b':' && b[raiz - 2].is_ascii_alphabetic();
        let fin_nombre = p + "secretos".len();
        let inicio = fin_de_tramo(b, fin_nombre, |c| !es_barra(c));
        if raiz < p && (en_etc || en_unidad) && inicio > fin_nombre {
            agregar(
                &mut tramos,
                texto,
                inicio,
                fin_de_valor(b, inicio, es_delimitador),
            );
        }
    }
    reemplazar(texto, &tramos)
}

/// `Bearer <token>`, con el token en la misma línea o en la siguiente.
fn portador(texto: &str) -> String {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let mut tramos = Vec::new();
    for p in apariciones(b, b"bearer") {
        let tras = p + "bearer".len();
        if !inicio_de_palabra(b, p) || !b.get(tras).is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        let inicio = saltar_espacios(b, tras);
        if inicio - p > ALCANCE {
            continue;
        }
        let fin = fin_de_valor(b, inicio, |c| es_delimitador(c) || b",;".contains(&c));
        agregar(&mut tramos, texto, inicio, fin);
    }
    reemplazar(texto, &tramos)
}

/// Lo que, contenido en el nombre de una clave, la vuelve sensible: `token`, `access_token`,
/// `MINIATURAS_CLAVE_API`, `db_password`, `pwd`, `clave_privada`…
///
/// `pwd` también tapa `PWD=/ruta`, la variable del directorio actual: es una sobrerredacción
/// asumida, a cambio de no dejar pasar una contraseña abreviada.
const CLAVES: [&str; 19] = [
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "passphrase",
    "contrasena",
    "contraseña",
    "clave",
    "credencial",
    "credential",
    "apikey",
    "api_key",
    "api-key",
    "private_key",
    "private-key",
    "privatekey",
    "authorization",
    "cookie",
];

/// Lo que puede formar el nombre de una clave. Los bytes no ASCII cuentan, para que
/// `contraseña` sea una sola palabra.
fn es_de_clave(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c >= 0x80
}

/// `clave=valor`, `clave: valor`, `"clave": "valor"`, con el valor en la misma línea o en la
/// siguiente.
///
/// - Con comillas, el valor llega hasta la comilla de cierre (respetando `\"`).
/// - Con `=` (estilo `clave=valor` o consulta de URL), hasta el primer espacio o separador.
/// - Con `:` y la clave sin comillas (estilo cabecera: `Authorization: Basic …`), hasta el
///   final de la línea.
fn valores_con_clave(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut tramos = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !es_de_clave(b[i]) {
            i += 1;
            continue;
        }
        let inicio_clave = i;
        i = fin_de_tramo(b, i, |c| !es_de_clave(c));
        if let Some((inicio, fin)) = valor_de_clave(texto, inicio_clave, i) {
            agregar(&mut tramos, texto, inicio, fin);
            i = i.max(fin);
        }
    }
    reemplazar(texto, &tramos)
}

/// Si la palabra `texto[inicio..fin]` es una clave sensible seguida de `=` o `:`, el tramo de
/// su valor.
fn valor_de_clave(texto: &str, inicio: usize, fin: usize) -> Option<(usize, usize)> {
    let b = texto.as_bytes();
    if fin - inicio > CLAVE_MAX {
        return None;
    }
    let mut k = fin;
    let citada = matches!(b.get(k), Some(b'"' | b'\''));
    if citada {
        k += 1;
    }
    k = saltar_espacios(b, k);
    let separador = match b.get(k) {
        Some(&c @ (b'=' | b':')) => c,
        _ => return None,
    };
    if separador == b'=' && b.get(k + 1) == Some(&b'=') {
        return None; // una comparación (`token == x`), no una asignación
    }
    k = saltar_espacios(b, k + 1);
    if k >= b.len() || k - inicio > ALCANCE {
        return None;
    }
    let palabra = texto[inicio..fin].to_lowercase();
    if !CLAVES.iter().any(|clave| palabra.contains(clave)) {
        return None;
    }
    let tramo = match b[k] {
        comilla @ (b'"' | b'\'') => (k + 1, fin_de_cita(b, k + 1, comilla)),
        _ if separador == b':' && !citada => (k, fin_de_linea(b, k)),
        _ => (
            k,
            fin_de_valor(b, k, |c| es_delimitador(c) || b",;&)]}".contains(&c)),
        ),
    };
    Some(tramo)
}

/// Hasta la comilla de cierre (saltando las escapadas) o el final de la línea.
fn fin_de_cita(b: &[u8], desde: usize, comilla: u8) -> usize {
    let mut k = desde;
    while k < b.len() && b[k] != comilla && b[k] != b'\n' {
        let escapa = b[k] == b'\\' && b.get(k + 1).is_some_and(|&c| c != b'\n');
        k += if escapa { 2 } else { 1 };
    }
    k.min(b.len())
}

/// Hasta el final de la línea, sin los espacios (ni el `\r`) del final.
fn fin_de_linea(b: &[u8], desde: usize) -> usize {
    let mut fin = fin_de_tramo(b, desde, |c| c == b'\n');
    while fin > desde && b[fin - 1].is_ascii_whitespace() {
        fin -= 1;
    }
    fin
}

/// Claves con prefijo conocido: `sk-…`, `sk_…`, `ghp_…`. Queda el prefijo, para saber qué
/// tipo de clave era.
fn claves_con_prefijo(texto: &str) -> String {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let mut encontrados = Vec::new();
    for prefijo in ["sk-", "sk_", "ghp_"] {
        for p in apariciones(b, prefijo.as_bytes()) {
            let inicio = p + prefijo.len();
            let fin = fin_de_valor(b, inicio, |c| {
                !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            });
            if inicio_de_palabra(b, p) && fin - inicio >= 8 {
                encontrados.push((inicio, fin));
            }
        }
    }
    encontrados.sort_unstable();
    let mut tramos = Vec::new();
    for (inicio, fin) in encontrados {
        agregar(&mut tramos, texto, inicio, fin);
    }
    reemplazar(texto, &tramos)
}

/// `CLI-123456` (cuatro dígitos o más) → `CLI-[REDACTADO]`.
fn identificadores_de_cuenta(texto: &str) -> String {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let mut tramos = Vec::new();
    for p in apariciones(b, b"cli-") {
        let inicio = p + "cli-".len();
        let fin = fin_de_valor(b, inicio, |c| !c.is_ascii_digit());
        if inicio_de_palabra(b, p) && fin - inicio >= 4 {
            agregar(&mut tramos, texto, inicio, fin);
        }
    }
    reemplazar(texto, &tramos)
}

/// Direcciones de correo: `alguien@ejemplo.test`, también con letras no ASCII.
fn correos(texto: &str) -> String {
    let b = texto.as_bytes();
    let es_local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c) || c >= 0x80;
    let es_dominio = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c >= 0x80;
    let mut tramos = Vec::new();
    for p in apariciones(b, b"@") {
        let inicio = fin_de_tramo_atras(b, p, es_local);
        let mut fin = fin_de_tramo(b, p + 1, |c| !es_dominio(c));
        while fin > p + 1 && matches!(b[fin - 1], b'.' | b'-') {
            fin -= 1;
        }
        if inicio < p && es_dominio_de_correo(&texto[p + 1..fin]) {
            agregar(&mut tramos, texto, inicio, fin);
        }
    }
    reemplazar(texto, &tramos)
}

/// Un dominio con al menos dos partes y una terminación de dos letras o más. `2x.png` no
/// cuenta: `miniatura@2x.png` es un archivo de alta densidad, no un correo.
fn es_dominio_de_correo(dominio: &str) -> bool {
    let Some((resto, terminacion)) = dominio.rsplit_once('.') else {
        return false;
    };
    let primera = resto.split('.').next().unwrap_or_default();
    let densidad = primera
        .strip_suffix(['x', 'X'])
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()));
    !resto.is_empty()
        && terminacion.len() >= 2
        && terminacion.bytes().all(|c| c.is_ascii_alphabetic())
        && !densidad
}

/// La cola de un registro, ya redactada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cola {
    /// Las últimas líneas, en orden.
    pub lineas: Vec<Redactado>,
    /// Si el archivo era más grande que el tope: entonces solo se muestran las líneas que
    /// empiezan dentro de la ventana.
    pub recortado: bool,
    /// Bytes que se leyeron del archivo: nunca más que el tope más [`CONTEXTO`].
    pub bytes_leidos: u64,
    /// Bytes que tiene el archivo.
    pub bytes_archivo: u64,
}

/// Las últimas `lineas` líneas de `ruta`, mirando solo sus últimos `tope` bytes.
///
/// Si el archivo es más grande que el tope, se leen además [`CONTEXTO`] bytes previos, se
/// redacta todo junto y recién entonces se descartan las líneas que empiezan antes de la
/// ventana (entre ellas, la que quedó cortada). Así una clave anterior al corte todavía
/// alcanza a su valor.
pub fn leer_cola(ruta: &Path, lineas: usize, tope: u64) -> io::Result<Cola> {
    let mut archivo = File::open(ruta)?;
    let largo = archivo.metadata()?.len();
    let ventana = largo.saturating_sub(tope);
    let desde = ventana.saturating_sub(CONTEXTO);
    archivo.seek(SeekFrom::Start(desde))?;
    let mut bytes = Vec::new();
    archivo.take(largo - desde).read_to_end(&mut bytes)?;

    let corte = usize::try_from(ventana - desde)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let descartar = lineas_que_empiezan_antes(&bytes, corte);
    let mut todas = redactar(&String::from_utf8_lossy(&bytes)).lineas();
    todas.drain(..descartar.min(todas.len()));
    if todas.len() > lineas {
        todas.drain(..todas.len() - lineas);
    }
    Ok(Cola {
        lineas: todas,
        recortado: ventana > 0,
        bytes_leidos: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        bytes_archivo: largo,
    })
}

/// Cuántas líneas de `bytes` empiezan antes de la posición `corte`. La primera empieza en 0;
/// cada una de las demás, después de un `\n`.
fn lineas_que_empiezan_antes(bytes: &[u8], corte: usize) -> usize {
    if corte == 0 {
        return 0;
    }
    1 + bytes[..corte - 1].iter().filter(|&&c| c == b'\n').count()
}
