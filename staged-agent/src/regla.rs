//! Reglas de normalización de texto.
//!
//! Cada regla es una función pura de texto en texto, e **idempotente**:
//! aplicarla dos veces da lo mismo que aplicarla una. Así un plan simulado
//! sobre un archivo ya normalizado sale vacío, y volver a correr el agente no
//! propone nada nuevo.

/// Una transformación de texto.
pub trait Regla {
    /// Nombre corto, para el plan.
    fn nombre(&self) -> &'static str;

    /// El texto normalizado. Tiene que ser idempotente.
    fn aplicar(&self, texto: &str) -> String;
}

/// Quita los CR que preceden a un LF: `\r\n` pasa a `\n`.
///
/// También quita los CR repetidos (`\r\r\n`), que deja una doble conversión.
/// Un CR que no precede a un LF no se toca.
#[derive(Debug, Clone, Copy, Default)]
pub struct FinesDeLinea;

impl Regla for FinesDeLinea {
    fn nombre(&self) -> &'static str {
        "finales de línea"
    }

    fn aplicar(&self, texto: &str) -> String {
        let mut salida = String::with_capacity(texto.len());
        for c in texto.chars() {
            if c == '\n' {
                while salida.ends_with('\r') {
                    salida.pop();
                }
            }
            salida.push(c);
        }
        salida
    }
}

/// Quita el blanco al final de cada línea: espacios, tabuladores y CR sueltos.
///
/// Los CR con los que termina la línea —los de un `\r\n`— se conservan:
/// cambiarlos es trabajo de [`FinesDeLinea`]. Un CR seguido de espacios no
/// termina nada, y se va con ellos; si se conservara, al quitar los espacios
/// quedaría pegado al LF y formaría un `\r\n` que antes no estaba.
#[derive(Debug, Clone, Copy, Default)]
pub struct EspaciosFinales;

impl Regla for EspaciosFinales {
    fn nombre(&self) -> &'static str {
        "espacios finales"
    }

    fn aplicar(&self, texto: &str) -> String {
        let lineas: Vec<String> = texto
            .split('\n')
            .map(|linea| {
                let fin_de_linea = &linea[linea.trim_end_matches('\r').len()..];
                let cuerpo = linea.trim_end_matches([' ', '\t', '\r']);
                format!("{cuerpo}{fin_de_linea}")
            })
            .collect();
        lineas.join("\n")
    }
}

/// Deja exactamente un fin de línea al final de un archivo que no está vacío.
///
/// Agrega el que falta y quita las líneas vacías sobrantes del final. Respeta
/// el estilo: si el último fin de línea era `\r\n`, deja `\r\n`. Un archivo
/// hecho solo de saltos de línea queda vacío.
#[derive(Debug, Clone, Copy, Default)]
pub struct SaltoFinal;

impl Regla for SaltoFinal {
    fn nombre(&self) -> &'static str {
        "salto final"
    }

    fn aplicar(&self, texto: &str) -> String {
        let cuerpo = texto.trim_end_matches(['\r', '\n']);
        if cuerpo.is_empty() {
            return String::new();
        }
        let fin = if texto.ends_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        format!("{cuerpo}{fin}")
    }
}

/// Las tres reglas, en el orden en que conviene aplicarlas: primero los
/// finales de línea, después los espacios y por último el salto final.
pub fn reglas_por_defecto() -> Vec<Box<dyn Regla>> {
    vec![
        Box::new(FinesDeLinea),
        Box::new(EspaciosFinales),
        Box::new(SaltoFinal),
    ]
}
