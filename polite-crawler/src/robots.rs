//! `robots.txt` según la RFC 9309.
//!
//! - Los grupos se eligen por **token de producto**, sin distinguir mayúsculas.
//!   Si varios grupos nombran al mismo agente, sus reglas se combinan. Si ninguno
//!   lo nombra, se usan los grupos `*`; si tampoco hay, todo está permitido.
//! - Las reglas `Allow` y `Disallow` admiten `*` (cualquier secuencia) y `$`
//!   (fin de la ruta). Gana la regla **más específica** — la de patrón más largo —
//!   y ante un empate gana `Allow`.
//! - `/robots.txt` está siempre permitido.
//! - `Crawl-delay` no es parte de la RFC, pero se honra: solo puede hacer al
//!   crawler más lento, nunca más rápido.
//!
//! Las rutas se comparan con los caracteres fuera de ASCII codificados en
//! porcentaje y el hexadecimal en mayúsculas, así `/ñ` y `/%c3%b1` son lo mismo.

use std::time::Duration;

/// Se parsean como mucho estos bytes del archivo (la RFC pide al menos 500 KiB).
pub const MAX_BYTES: usize = 500 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Regla {
    permite: bool,
    patron: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Grupo {
    agentes: Vec<String>,
    reglas: Vec<Regla>,
    crawl_delay: Option<f64>,
}

/// Un `robots.txt` parseado.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Robots {
    grupos: Vec<Grupo>,
}

/// Las reglas que le tocan a un agente concreto.
#[derive(Debug, Clone, PartialEq)]
pub struct Politica {
    reglas: Vec<Regla>,
    /// Pausa pedida entre peticiones, si el grupo la declara.
    pub crawl_delay: Option<Duration>,
}

/// La decisión sobre una ruta, con la regla que la tomó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// `true` si se puede visitar.
    pub permitida: bool,
    /// La regla que decidió, como aparece en el archivo (p. ej. `Disallow: /privado`).
    pub regla: Option<String>,
}

impl Robots {
    /// Parsea el contenido de un `robots.txt`. Nunca falla: lo que no entiende, lo ignora.
    pub fn parsear(texto: &str) -> Self {
        let texto = recortar(texto, MAX_BYTES);
        let mut grupos: Vec<Grupo> = Vec::new();
        let mut actual = Grupo::default();
        let mut leyendo_agentes = false;

        for linea in texto.lines() {
            let linea = linea.split('#').next().unwrap_or("").trim();
            let Some((clave, valor)) = linea.split_once(':') else { continue };
            let valor = valor.trim();
            match clave.trim().to_ascii_lowercase().as_str() {
                "user-agent" => {
                    // Un `User-agent` después de reglas abre un grupo nuevo.
                    if !leyendo_agentes && !actual.agentes.is_empty() {
                        grupos.push(std::mem::take(&mut actual));
                    }
                    actual.agentes.push(token_de(valor));
                    leyendo_agentes = true;
                }
                "allow" | "disallow" if !actual.agentes.is_empty() => {
                    leyendo_agentes = false;
                    // Un `Disallow:` vacío significa "nada prohibido": no es una regla.
                    if !valor.is_empty() {
                        let permite = clave.trim().eq_ignore_ascii_case("allow");
                        actual.reglas.push(Regla { permite, patron: normalizar(valor) });
                    }
                }
                "crawl-delay" if !actual.agentes.is_empty() => {
                    leyendo_agentes = false;
                    if let Ok(s) = valor.parse::<f64>() {
                        if s.is_finite() && s >= 0.0 {
                            actual.crawl_delay = Some(actual.crawl_delay.map_or(s, |d| d.max(s)));
                        }
                    }
                }
                _ => {}
            }
        }
        if !actual.agentes.is_empty() {
            grupos.push(actual);
        }
        Self { grupos }
    }

    /// Todo permitido (lo que dice la RFC cuando `robots.txt` no existe).
    pub fn permitir_todo() -> Self {
        Self::default()
    }

    /// Todo prohibido (lo que dice la RFC cuando `robots.txt` es inalcanzable).
    pub fn prohibir_todo() -> Self {
        Self {
            grupos: vec![Grupo {
                agentes: vec!["*".into()],
                reglas: vec![Regla { permite: false, patron: "/".into() }],
                crawl_delay: None,
            }],
        }
    }

    /// Las reglas que le tocan al agente con este token de producto.
    pub fn politica_para(&self, token: &str) -> Politica {
        let token = token.to_ascii_lowercase();
        let propios: Vec<&Grupo> = self.grupos.iter().filter(|g| g.agentes.contains(&token)).collect();
        let elegidos = if propios.is_empty() {
            self.grupos.iter().filter(|g| g.agentes.iter().any(|a| a == "*")).collect()
        } else {
            propios
        };
        Politica {
            reglas: elegidos.iter().flat_map(|g| g.reglas.iter().cloned()).collect(),
            crawl_delay: elegidos
                .iter()
                .filter_map(|g| g.crawl_delay)
                .reduce(f64::max)
                .map(Duration::from_secs_f64),
        }
    }
}

impl Politica {
    /// Decide sobre una ruta con su consulta (`/buscar?q=x`).
    pub fn decidir(&self, ruta: &str) -> Decision {
        let ruta = normalizar(ruta);
        if ruta == "/robots.txt" {
            return Decision { permitida: true, regla: None };
        }
        let ganadora = self
            .reglas
            .iter()
            .filter(|r| coincide(&r.patron, &ruta))
            // Más largo gana; ante un empate, Allow (true > false).
            .max_by_key(|r| (r.patron.len(), r.permite));
        match ganadora {
            Some(r) => Decision {
                permitida: r.permite,
                regla: Some(format!("{}: {}", if r.permite { "Allow" } else { "Disallow" }, r.patron)),
            },
            None => Decision { permitida: true, regla: None },
        }
    }
}

/// El token de producto de una línea `User-agent`: el nombre sin la versión.
fn token_de(valor: &str) -> String {
    valor.split('/').next().unwrap_or("").trim().to_ascii_lowercase()
}

fn recortar(texto: &str, max: usize) -> &str {
    if texto.len() <= max {
        return texto;
    }
    let mut corte = max;
    while !texto.is_char_boundary(corte) {
        corte -= 1;
    }
    &texto[..corte]
}

/// Codifica en porcentaje lo que no es ASCII y pasa a mayúsculas el hex existente.
fn normalizar(ruta: &str) -> String {
    let mut salida = String::with_capacity(ruta.len());
    let bytes = ruta.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let es_escape = b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit();
        if es_escape {
            salida.push('%');
            salida.push(bytes[i + 1].to_ascii_uppercase() as char);
            salida.push(bytes[i + 2].to_ascii_uppercase() as char);
            i += 3;
        } else if b.is_ascii() {
            salida.push(b as char);
            i += 1;
        } else {
            salida.push_str(&format!("%{b:02X}"));
            i += 1;
        }
    }
    salida
}

/// ¿El patrón (con `*` y `$`) coincide con la ruta, anclado al comienzo?
fn coincide(patron: &str, ruta: &str) -> bool {
    let (patron, anclado) = match patron.strip_suffix('$') {
        Some(p) => (p, true),
        None => (patron, false),
    };
    let partes: Vec<&str> = patron.split('*').collect();
    let Some((primera, resto)) = partes.split_first() else { return false };
    if !ruta.starts_with(primera) {
        return false;
    }
    let mut pos = primera.len();
    let Some((ultima, medio)) = resto.split_last() else {
        // Sin comodines: prefijo, o igualdad exacta si está anclado.
        return !anclado || ruta.len() == pos;
    };
    for parte in medio {
        match ruta[pos..].find(parte) {
            Some(i) => pos += i + parte.len(),
            None => return false,
        }
    }
    if anclado {
        ruta.len() >= pos + ultima.len() && ruta.ends_with(ultima)
    } else {
        ruta[pos..].contains(ultima)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comodines_y_ancla() {
        assert!(coincide("/fish", "/fish.html"));
        assert!(!coincide("/fish$", "/fish.html"));
        assert!(coincide("/fish$", "/fish"));
        assert!(coincide("/*.php", "/index.php"));
        assert!(coincide("/*.php", "/carpeta/archivo.php?x=1"));
        assert!(coincide("/*.php$", "/archivo.php"));
        assert!(!coincide("/*.php$", "/archivo.php?x=1"));
        assert!(coincide("/a*b*c", "/aXXbYYc"));
        assert!(!coincide("/a*b*c", "/aXXcYYb"));
        assert!(!coincide("/pez", "/Pez"), "las rutas distinguen mayúsculas");
    }

    #[test]
    fn la_normalizacion_iguala_utf8_y_porcentaje() {
        assert_eq!(normalizar("/ñ"), "/%C3%B1");
        assert_eq!(normalizar("/%c3%b1"), "/%C3%B1");
        assert_eq!(normalizar("/100%"), "/100%");
    }
}
