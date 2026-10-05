//! Filtro de basura: barato, y por eso va primero.
//!
//! Descarta páginas vacías, granjas de enlaces y texto repetido antes de gastar
//! en escanear inyecciones o destilar. Los umbrales por defecto son razonables
//! para artículos; se ajustan en [`UmbralesBasura`].

use crate::html::Extraido;

/// Umbrales del filtro de basura.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UmbralesBasura {
    /// Mínimo de caracteres de texto visible (sin espacios).
    pub min_caracteres: usize,
    /// Mínimo de palabras visibles.
    pub min_palabras: usize,
    /// Máxima fracción del texto visible que puede estar dentro de enlaces.
    pub max_fraccion_enlaces: f64,
    /// Máxima fracción de líneas que pueden ser repeticiones de otra.
    pub max_fraccion_repetida: f64,
}

impl Default for UmbralesBasura {
    fn default() -> Self {
        Self {
            min_caracteres: 280,
            min_palabras: 50,
            max_fraccion_enlaces: 0.5,
            max_fraccion_repetida: 0.5,
        }
    }
}

/// Medidas del documento, para decidir y para explicar la decisión.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Medidas {
    /// Caracteres visibles sin espacios.
    pub caracteres: usize,
    /// Palabras visibles.
    pub palabras: usize,
    /// Fracción del texto visible dentro de enlaces.
    pub fraccion_enlaces: f64,
    /// Fracción de líneas no vacías que repiten otra anterior.
    pub fraccion_repetida: f64,
}

/// Mide el texto extraído.
pub fn medir(e: &Extraido) -> Medidas {
    let caracteres = e.visible.chars().filter(|c| !c.is_whitespace()).count();
    let palabras = e.visible.split_whitespace().count();
    let fraccion_enlaces = if caracteres == 0 {
        0.0
    } else {
        e.caracteres_en_enlaces as f64 / caracteres as f64
    };

    let lineas: Vec<&str> = e.visible.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut vistas = std::collections::HashSet::new();
    let repetidas = lineas.iter().filter(|l| !vistas.insert(l.trim())).count();
    let fraccion_repetida = if lineas.is_empty() {
        0.0
    } else {
        repetidas as f64 / lineas.len() as f64
    };

    Medidas {
        caracteres,
        palabras,
        fraccion_enlaces,
        fraccion_repetida,
    }
}

/// `Some(motivo)` si el documento es basura según `u`.
pub fn motivo_de_basura(m: &Medidas, u: &UmbralesBasura) -> Option<String> {
    if m.caracteres < u.min_caracteres {
        return Some(format!(
            "muy poco texto: {} caracteres (mínimo {})",
            m.caracteres, u.min_caracteres
        ));
    }
    if m.palabras < u.min_palabras {
        return Some(format!(
            "muy pocas palabras: {} (mínimo {})",
            m.palabras, u.min_palabras
        ));
    }
    if m.fraccion_enlaces > u.max_fraccion_enlaces {
        return Some(format!(
            "granja de enlaces: {:.0} % del texto está en enlaces (máximo {:.0} %)",
            m.fraccion_enlaces * 100.0,
            u.max_fraccion_enlaces * 100.0
        ));
    }
    if m.fraccion_repetida > u.max_fraccion_repetida {
        return Some(format!(
            "texto repetido: {:.0} % de las líneas se repiten (máximo {:.0} %)",
            m.fraccion_repetida * 100.0,
            u.max_fraccion_repetida * 100.0
        ));
    }
    None
}
