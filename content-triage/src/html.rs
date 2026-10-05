//! De HTML a texto, separando lo visible de lo oculto.
//!
//! No es un parser HTML5 completo: es un recorrido de estados suficiente para
//! extraer texto de páginas reales. Ante HTML malformado falla hacia el lado
//! seguro — un elemento oculto sin cerrar deja todo lo que sigue como oculto
//! (se escanea, pero no se entrega), y un `<script>` sin cerrar se descarta
//! hasta el final.

/// Texto extraído de un documento.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraido {
    /// Lo que un lector vería en pantalla.
    pub visible: String,
    /// Lo que está en el documento pero oculto a la vista (`display:none`,
    /// `hidden`, `aria-hidden`, tamaño u opacidad cero). Se escanea igual.
    pub oculto: String,
    /// Caracteres de texto visible que están dentro de un enlace.
    pub caracteres_en_enlaces: usize,
    /// Cuántos elementos ocultos con texto se encontraron.
    pub elementos_ocultos: usize,
}

/// Elementos cuyo contenido nunca es texto para el lector.
const SIN_TEXTO: &[&str] = &[
    "head", "script", "style", "noscript", "template", "svg", "iframe",
];

/// Elementos que no tienen cierre.
const VACIOS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elementos que cortan la línea al abrirse o cerrarse.
const DE_BLOQUE: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "tr",
    "td",
    "th",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "blockquote",
    "nav",
    "main",
    "aside",
];

struct Abierto {
    nombre: String,
    oculto: bool,
    enlace: bool,
}

/// Extrae el texto de `html`.
pub fn extraer(html: &str) -> Extraido {
    let mut salida = Extraido::default();
    let mut pila: Vec<Abierto> = Vec::new();
    let mut resto = html;

    while !resto.is_empty() {
        let Some(inicio) = resto.find('<') else {
            agregar_texto(&mut salida, &pila, resto);
            break;
        };
        agregar_texto(&mut salida, &pila, &resto[..inicio]);
        resto = &resto[inicio..];

        if let Some(despues) = resto.strip_prefix("<!--") {
            resto = match despues.find("-->") {
                Some(fin) => &despues[fin + 3..],
                None => "",
            };
            continue;
        }

        // `a < b` no es una etiqueta: tras el `<` tiene que venir una letra, `/`, `!` o `?`.
        let es_etiqueta = resto[1..]
            .starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?'));
        let Some(fin) = resto.find('>').filter(|_| es_etiqueta) else {
            agregar_texto(&mut salida, &pila, "<");
            resto = &resto[1..];
            continue;
        };
        let etiqueta = &resto[1..fin];
        resto = &resto[fin + 1..];

        if let Some(nombre) = etiqueta.strip_prefix('/') {
            let nombre = nombre_de(nombre);
            if DE_BLOQUE.contains(&nombre.as_str()) {
                salto(&mut salida, &pila);
            }
            if let Some(pos) = pila.iter().rposition(|a| a.nombre == nombre) {
                pila.truncate(pos);
            }
            continue;
        }
        if etiqueta.starts_with('!') || etiqueta.starts_with('?') {
            continue; // doctype, instrucciones de procesamiento
        }

        let nombre = nombre_de(etiqueta);

        if SIN_TEXTO.contains(&nombre.as_str()) {
            let cierre = format!("</{nombre}");
            resto = match resto.to_ascii_lowercase().find(&cierre) {
                Some(pos) => {
                    let tras = &resto[pos..];
                    match tras.find('>') {
                        Some(f) => &tras[f + 1..],
                        None => "",
                    }
                }
                None => "",
            };
            continue;
        }

        if DE_BLOQUE.contains(&nombre.as_str()) {
            salto(&mut salida, &pila);
        }
        if VACIOS.contains(&nombre.as_str()) || etiqueta.trim_end().ends_with('/') {
            continue;
        }

        let oculto = esta_oculto(etiqueta);
        if oculto && !pila.iter().any(|a| a.oculto) {
            salida.elementos_ocultos += 1;
        }
        pila.push(Abierto {
            enlace: nombre == "a",
            nombre,
            oculto,
        });
    }

    salida.visible = compactar(&salida.visible);
    salida.oculto = compactar(&salida.oculto);
    salida
}

fn nombre_de(etiqueta: &str) -> String {
    etiqueta
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Las formas habituales de esconder texto a la vista sin sacarlo del documento.
fn esta_oculto(etiqueta: &str) -> bool {
    let e = etiqueta.to_ascii_lowercase();
    let sin_espacios: String = e.chars().filter(|c| !c.is_whitespace()).collect();

    let atributo_hidden = e
        .split(|c: char| c.is_whitespace() || c == '/')
        .skip(1)
        .any(|parte| parte == "hidden" || parte.starts_with("hidden="));

    atributo_hidden
        || sin_espacios.contains("aria-hidden=\"true\"")
        || sin_espacios.contains("aria-hidden='true'")
        || sin_espacios.contains("display:none")
        || sin_espacios.contains("visibility:hidden")
        || propiedad_en_cero(&sin_espacios, "font-size:0")
        || propiedad_en_cero(&sin_espacios, "opacity:0")
}

/// `font-size:0` oculta; `font-size:0.9em` no. El cero tiene que terminar ahí
/// (o seguir con una unidad), no ser el comienzo de un decimal.
fn propiedad_en_cero(css: &str, patron: &str) -> bool {
    css.match_indices(patron).any(|(i, _)| {
        let resto = &css[i + patron.len()..];
        !resto.starts_with(|c: char| c == '.' || c.is_ascii_digit())
    })
}

fn agregar_texto(salida: &mut Extraido, pila: &[Abierto], crudo: &str) {
    if crudo.is_empty() {
        return;
    }
    let texto = decodificar_entidades(crudo);
    if pila.iter().any(|a| a.oculto) {
        salida.oculto.push_str(&texto);
        salida.oculto.push(' ');
    } else {
        if pila.iter().any(|a| a.enlace) {
            salida.caracteres_en_enlaces += texto.chars().filter(|c| !c.is_whitespace()).count();
        }
        salida.visible.push_str(&texto);
    }
}

fn salto(salida: &mut Extraido, pila: &[Abierto]) {
    if !pila.iter().any(|a| a.oculto) {
        salida.visible.push('\n');
    }
}

/// Decodifica las entidades con nombre más comunes y todas las numéricas.
/// Una entidad desconocida queda tal cual: es texto, no se inventa nada.
pub fn decodificar_entidades(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(i) = resto.find('&') {
        salida.push_str(&resto[..i]);
        resto = &resto[i..];
        // Se busca el `;` por caracteres, no por bytes: cortar en un byte fijo
        // podría caer en medio de una letra acentuada.
        let Some(fin) = resto
            .char_indices()
            .take_while(|(i, _)| *i < 12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i)
        else {
            salida.push('&');
            resto = &resto[1..];
            continue;
        };
        let cuerpo = &resto[1..fin];
        let decodificado = match cuerpo {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => cuerpo
                .strip_prefix("#x")
                .or_else(|| cuerpo.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| cuerpo.strip_prefix('#').and_then(|d| d.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match decodificado {
            Some(c) => {
                salida.push(c);
                resto = &resto[fin + 1..];
            }
            None => {
                salida.push('&');
                resto = &resto[1..];
            }
        }
    }
    salida.push_str(resto);
    salida
}

/// Colapsa espacios dentro de cada línea y descarta líneas vacías repetidas.
fn compactar(texto: &str) -> String {
    let mut lineas: Vec<String> = Vec::new();
    for linea in texto.lines() {
        let limpia = linea.split_whitespace().collect::<Vec<_>>().join(" ");
        if limpia.is_empty() && lineas.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lineas.push(limpia);
    }
    while lineas.last().is_some_and(|l| l.is_empty()) {
        lineas.pop();
    }
    lineas.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separa_lo_visible_de_lo_oculto() {
        let e = extraer(
            "<p>Hola <b>mundo</b></p><div style=\"display: none\">secreto</div>\
             <span hidden>otro</span><p aria-hidden=\"true\">más</p>",
        );
        assert_eq!(e.visible, "Hola mundo");
        assert!(
            e.oculto.contains("secreto") && e.oculto.contains("otro") && e.oculto.contains("más")
        );
        assert_eq!(e.elementos_ocultos, 3);
    }

    #[test]
    fn descarta_scripts_estilos_y_comentarios() {
        let e = extraer(
            "<style>p{}</style><script>alert(1)</script><!-- nota -->texto<SCRIPT>x</SCRIPT>",
        );
        assert_eq!(e.visible, "texto");
    }

    #[test]
    fn cuenta_el_texto_de_los_enlaces() {
        let e = extraer("<p>uno dos <a href=\"#\">tres</a></p>");
        assert_eq!(e.caracteres_en_enlaces, 4);
    }

    #[test]
    fn un_oculto_sin_cerrar_oculta_el_resto() {
        let e = extraer("visible<div style=\"display:none\">oculto<p>sigue oculto");
        assert_eq!(e.visible, "visible");
        assert!(e.oculto.contains("sigue oculto"));
    }

    #[test]
    fn decodifica_entidades_con_nombre_y_numericas() {
        assert_eq!(
            decodificar_entidades("a &amp; b &lt;x&gt; &#105;&#x67; &raro; &"),
            "a & b <x> ig &raro; &"
        );
    }

    #[test]
    fn un_menor_que_suelto_es_texto() {
        assert_eq!(
            extraer("<p>si a < b y c > d</p>").visible,
            "si a < b y c > d"
        );
        assert_eq!(extraer("fin <").visible, "fin <");
    }

    #[test]
    fn una_entidad_junto_a_acentos_no_rompe() {
        assert_eq!(
            decodificar_entidades("&áéíóúñ sin cierre"),
            "&áéíóúñ sin cierre"
        );
        assert_eq!(decodificar_entidades("canción &amp; más"), "canción & más");
    }

    #[test]
    fn un_tamano_decimal_no_es_un_texto_oculto() {
        let e = extraer("<p style=\"font-size: 0.9em; opacity: 0.8\">se ve</p><p style=\"font-size:0px\">no</p>");
        assert_eq!(e.visible, "se ve");
        assert_eq!(e.oculto, "no");
    }

    #[test]
    fn el_head_no_es_texto_del_cuerpo() {
        assert_eq!(
            extraer("<head><title>Título</title></head><body>cuerpo</body>").visible,
            "cuerpo"
        );
    }

    #[test]
    fn una_palabra_que_contiene_hidden_no_oculta() {
        let e = extraer("<p class=\"unhidden-note\">se ve</p>");
        assert_eq!(e.visible, "se ve");
    }
}
