//! Normalización previa al escaneo.
//!
//! Un ataque se esconde en la codificación antes que en las palabras: letras de
//! ancho completo, caracteres invisibles entre letras, una `о` cirílica en
//! lugar de la `o` latina. Por eso esto corre **antes** de buscar patrones.
//!
//! Se producen dos textos a propósito:
//!
//! - `entregable`: NFKC y sin caracteres invisibles. Es lo que puede llegar al
//!   modelo. Una página legítima en ruso o en griego sigue intacta.
//! - `escaneable`: además, con los homoglifos plegados a su letra latina, sin
//!   tildes, en minúsculas y con los espacios colapsados. **Solo** sirve para
//!   buscar patrones; nunca se entrega.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Resultado de normalizar un texto.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Normalizado {
    /// NFKC, sin invisibles. Apto para entregar.
    pub entregable: String,
    /// Plegado agresivo, solo para escanear.
    pub escaneable: String,
    /// Caracteres invisibles o de control quitados.
    pub invisibles: usize,
    /// Palabras que mezclan letras latinas con homoglifos de otro alfabeto.
    pub palabras_mezcladas: usize,
}

/// Caracteres que no se ven pero separan letras: el atacante los mete en medio
/// de una frase para que una búsqueda literal no la encuentre.
fn es_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' // guion blando
            | '\u{200B}'..='\u{200F}' // ancho cero y marcas de dirección
            | '\u{202A}'..='\u{202E}' // incrustaciones bidireccionales
            | '\u{2060}'..='\u{2064}' // unión de palabras e invisibles matemáticos
            | '\u{2066}'..='\u{2069}' // aislamientos bidireccionales
            | '\u{FEFF}' // marca de orden de bytes
    ) || (c.is_control() && c != '\n' && c != '\t')
}

/// Letras de otros alfabetos que se ven idénticas a una latina.
fn homoglifo(c: char) -> Option<char> {
    Some(match c {
        // Cirílico
        'а' => 'a',
        'е' => 'e',
        'о' => 'o',
        'р' => 'p',
        'с' => 'c',
        'у' => 'y',
        'х' => 'x',
        'і' => 'i',
        'ј' => 'j',
        'ѕ' => 's',
        'ԁ' => 'd',
        'һ' => 'h',
        'ӏ' => 'l',
        'ԛ' => 'q',
        'ԝ' => 'w',
        'А' => 'a',
        'В' => 'b',
        'Е' => 'e',
        'К' => 'k',
        'М' => 'm',
        'Н' => 'h',
        'О' => 'o',
        'Р' => 'p',
        'С' => 'c',
        'Т' => 't',
        'Х' => 'x',
        'І' => 'i',
        'Ј' => 'j',
        'Ѕ' => 's',
        // Griego
        'α' => 'a',
        'ο' => 'o',
        'ν' => 'v',
        'ρ' => 'p',
        'ι' => 'i',
        'κ' => 'k',
        'τ' => 't',
        'Α' => 'a',
        'Β' => 'b',
        'Ε' => 'e',
        'Ζ' => 'z',
        'Η' => 'h',
        'Ι' => 'i',
        'Κ' => 'k',
        'Μ' => 'm',
        'Ν' => 'n',
        'Ο' => 'o',
        'Ρ' => 'p',
        'Τ' => 't',
        'Υ' => 'y',
        'Χ' => 'x',
        _ => return None,
    })
}

/// Minúscula sin marcas diacríticas: `Á` → `a`, `ñ` → `n`. Así una frase se
/// busca una sola vez, con o sin tildes.
pub fn sin_tildes(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase()
        .flat_map(|m| std::iter::once(m).nfd())
        .filter(|&d| !is_combining_mark(d))
}

/// Normaliza `texto` y cuenta las señales de ofuscación.
pub fn normalizar(texto: &str) -> Normalizado {
    let mut invisibles = 0;
    let entregable: String = texto
        .nfkc()
        .filter(|&c| {
            let fuera = es_invisible(c);
            if fuera {
                invisibles += 1;
            }
            !fuera
        })
        .collect();

    let mut palabras_mezcladas = 0;
    let mut escaneable = String::with_capacity(entregable.len());
    for palabra in entregable.split_whitespace() {
        let tiene_latina = palabra.chars().any(|c| c.is_ascii_alphabetic());
        let tiene_homoglifo = palabra.chars().any(|c| homoglifo(c).is_some());
        if tiene_latina && tiene_homoglifo {
            palabras_mezcladas += 1;
        }
        if !escaneable.is_empty() {
            escaneable.push(' ');
        }
        for c in palabra.chars() {
            match homoglifo(c) {
                Some(latina) => escaneable.push(latina),
                None => escaneable.extend(sin_tildes(c)),
            }
        }
    }

    Normalizado {
        entregable,
        escaneable,
        invisibles,
        palabras_mezcladas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quita_invisibles_y_los_cuenta() {
        let n = normalizar("ig\u{200B}no\u{200D}re");
        assert_eq!(n.entregable, "ignore");
        assert_eq!(n.invisibles, 2);
    }

    #[test]
    fn nfkc_pliega_ancho_completo_y_ligaduras() {
        assert_eq!(normalizar("ＩＧＮＯＲＥ ﬁle").entregable, "IGNORE file");
    }

    #[test]
    fn los_homoglifos_se_pliegan_solo_en_la_copia_de_escaneo() {
        // "ignоre" con una о cirílica.
        let n = normalizar("Ignоre this");
        assert_eq!(n.escaneable, "ignore this");
        assert!(n.entregable.contains('о'), "el texto entregable no se toca");
        assert_eq!(n.palabras_mezcladas, 1);
    }

    #[test]
    fn la_copia_de_escaneo_no_tiene_tildes() {
        assert_eq!(
            normalizar("Actúa como INSTRUCCIÓN").escaneable,
            "actua como instruccion"
        );
    }

    #[test]
    fn un_texto_en_otro_alfabeto_no_cuenta_como_mezcla() {
        let n = normalizar("Привет мир");
        assert_eq!(n.palabras_mezcladas, 0);
        assert_eq!(n.entregable, "Привет мир");
    }
}
