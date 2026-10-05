//! Quién es el crawler, dicho con claridad.
//!
//! La única forma de armar el `User-Agent` es [`Identidad::nueva`], y exige
//! tres cosas: un nombre de producto, una versión y **un contacto** (una URL o
//! un correo) para que quien administra un sitio sepa quién lo está leyendo y
//! cómo pedir que pare. El resultado tiene la forma habitual de un bot honesto:
//!
//! ```text
//! MiLector/1.2 (+https://ejemplo.test/bot)
//! ```
//!
//! El nombre no puede hacerse pasar por un navegador ni por el bot conocido de
//! otra organización, y la identidad no cambia durante la vida del crawler.

use std::fmt;

/// Nombres que no se pueden usar: presentarse como ellos sería suplantarlos.
const NOMBRES_AJENOS: &[&str] = &[
    "mozilla",
    "chrome",
    "safari",
    "firefox",
    "edge",
    "opera",
    "googlebot",
    "bingbot",
    "applebot",
    "duckduckbot",
    "yandex",
    "baiduspider",
];

/// Por qué una identidad no es válida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorIdentidad {
    /// El nombre tiene que ser 1–32 letras, `_` o `-`.
    Nombre(String),
    /// El nombre imita a un navegador o al bot de otro.
    NombreAjeno(String),
    /// La versión tiene que ser 1–16 letras, dígitos o puntos.
    Version(String),
    /// El contacto tiene que ser una URL `http(s)://` o un correo.
    Contacto(String),
}

impl fmt::Display for ErrorIdentidad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nombre(n) => write!(
                f,
                "nombre de producto inválido `{n}`: 1 a 32 letras, `_` o `-`"
            ),
            Self::NombreAjeno(n) => write!(
                f,
                "`{n}` imita a un navegador o al bot de otro: usa un nombre propio"
            ),
            Self::Version(v) => {
                write!(f, "versión inválida `{v}`: 1 a 16 letras, dígitos o puntos")
            }
            Self::Contacto(c) => {
                write!(f, "contacto inválido `{c}`: una URL http(s):// o un correo")
            }
        }
    }
}

impl std::error::Error for ErrorIdentidad {}

/// La identidad del crawler: cómo se presenta en cada petición.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identidad {
    token: String,
    cadena: String,
}

impl Identidad {
    /// Arma y valida una identidad.
    pub fn nueva(nombre: &str, version: &str, contacto: &str) -> Result<Self, ErrorIdentidad> {
        let nombre_ok = (1..=32).contains(&nombre.len())
            && nombre
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-');
        if !nombre_ok {
            return Err(ErrorIdentidad::Nombre(nombre.into()));
        }
        let bajo = nombre.to_ascii_lowercase();
        if NOMBRES_AJENOS.iter().any(|a| bajo.contains(a)) {
            return Err(ErrorIdentidad::NombreAjeno(nombre.into()));
        }

        let version_ok = (1..=16).contains(&version.len())
            && version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.');
        if !version_ok {
            return Err(ErrorIdentidad::Version(version.into()));
        }

        let imprimible = !contacto.is_empty()
            && contacto.len() <= 200
            && contacto
                .chars()
                .all(|c| c.is_ascii_graphic() && c != '(' && c != ')');
        let parece_url = contacto.starts_with("https://") || contacto.starts_with("http://");
        let parece_correo = contacto
            .split_once('@')
            .is_some_and(|(u, d)| !u.is_empty() && d.contains('.'));
        if !imprimible || !(parece_url || parece_correo) {
            return Err(ErrorIdentidad::Contacto(contacto.into()));
        }

        Ok(Self {
            token: bajo,
            cadena: format!("{nombre}/{version} (+{contacto})"),
        })
    }

    /// El token de producto, en minúsculas: con él se eligen las reglas de `robots.txt`.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// La cadena completa que viaja en el encabezado `User-Agent`.
    pub fn user_agent(&self) -> &str {
        &self.cadena
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_identidad_valida_tiene_la_forma_de_un_bot_honesto() {
        let i = Identidad::nueva("MiLector", "1.2", "https://ejemplo.test/bot").expect("válida");
        assert_eq!(i.user_agent(), "MiLector/1.2 (+https://ejemplo.test/bot)");
        assert_eq!(i.token(), "milector");
        assert!(Identidad::nueva("Lector_Docs", "0.1", "equipo@ejemplo.test").is_ok());
    }

    #[test]
    fn no_se_puede_prescindir_del_contacto() {
        for malo in [
            "",
            "nadie",
            "ftp://x.test",
            "https://x.test/(a)",
            "sin espacio@ejemplo.test",
        ] {
            assert!(
                matches!(
                    Identidad::nueva("Lector", "1", malo),
                    Err(ErrorIdentidad::Contacto(_))
                ),
                "`{malo}`"
            );
        }
    }

    #[test]
    fn no_se_puede_hacer_pasar_por_un_navegador_ni_por_otro_bot() {
        for ajeno in ["Mozilla", "MiChromeLector", "Googlebot", "BINGBOT"] {
            assert!(
                matches!(
                    Identidad::nueva(ajeno, "1", "https://x.test"),
                    Err(ErrorIdentidad::NombreAjeno(_))
                ),
                "`{ajeno}`"
            );
        }
    }

    #[test]
    fn nombre_y_version_se_validan() {
        assert!(matches!(
            Identidad::nueva("Mi Lector", "1", "https://x.test"),
            Err(ErrorIdentidad::Nombre(_))
        ));
        assert!(matches!(
            Identidad::nueva("Lector/2", "1", "https://x.test"),
            Err(ErrorIdentidad::Nombre(_))
        ));
        assert!(matches!(
            Identidad::nueva("Lector", "1 beta", "https://x.test"),
            Err(ErrorIdentidad::Version(_))
        ));
    }
}
