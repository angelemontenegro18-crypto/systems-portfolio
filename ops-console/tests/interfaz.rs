//! La interfaz no deja una puerta abierta a la inyección: un escáner revisa cada archivo de
//! `web/` —exactamente lo que se incrusta en el binario— en busca de lo que la permitiría
//! (HTML crudo, `eval`, scripts, estilos o manejadores en línea, recursos externos). Una
//! meta-prueba comprueba que el escáner de verdad encuentra cada una de esas cosas.
//!
//! El escáner no distingue comentarios: lo prohibido tampoco se puede nombrar en uno.

use std::collections::BTreeSet;

use ops_console::estaticos::RECURSOS;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hallazgo {
    regla: &'static str,
    detalle: String,
}

fn hallazgo(regla: &'static str, detalle: impl Into<String>) -> Hallazgo {
    Hallazgo {
        regla,
        detalle: detalle.into(),
    }
}

/// Lo que no puede aparecer en el código JavaScript.
const PROHIBIDO_EN_JS: [(&str, &str); 22] = [
    ("sin_html_crudo", "innerHTML"),
    ("sin_html_crudo", "outerHTML"),
    ("sin_html_crudo", "insertAdjacentHTML"),
    ("sin_html_crudo", "createContextualFragment"),
    ("sin_html_crudo", "DOMParser"),
    ("sin_html_crudo", "srcdoc"),
    ("sin_document_write", "document.write"),
    ("sin_eval", "eval("),
    ("sin_eval", "Function("),
    ("sin_eval", "setTimeout(\""),
    ("sin_eval", "setTimeout('"),
    ("sin_eval", "setTimeout(`"),
    ("sin_eval", "setInterval(\""),
    ("sin_eval", "setInterval('"),
    ("sin_eval", "setInterval(`"),
    ("sin_estilos_en_linea", ".style"),
    ("sin_estilos_en_linea", "cssText"),
    ("sin_estilos_en_linea", "setAttribute(\"style\""),
    ("sin_estilos_en_linea", "setAttribute('style'"),
    ("sin_manejadores_en_linea", "setAttribute(\"on"),
    ("sin_manejadores_en_linea", "setAttribute('on"),
    ("sin_import_dinamico", "import("),
];

/// El único `http://` admitido: el espacio de nombres de SVG, que no se descarga.
const ESPACIO_SVG: &str = "xmlns=\"http://www.w3.org/2000/svg\"";

fn todas(pajar: &str, aguja: &str) -> Vec<usize> {
    pajar.match_indices(aguja).map(|(i, _)| i).collect()
}

/// Atributos `on…=` dentro de etiquetas (`onclick=`, `onerror =`…).
fn manejadores_en_linea(texto: &str) -> Vec<String> {
    let bajo = texto.to_ascii_lowercase();
    let b = bajo.as_bytes();
    let mut encontrados = Vec::new();
    for i in todas(&bajo, "on") {
        let antes_es_espacio = i > 0 && (b[i - 1].is_ascii_whitespace() || b[i - 1] == b'/');
        let fin_nombre = b[i + 2..]
            .iter()
            .position(|c| !c.is_ascii_lowercase())
            .map_or(b.len(), |p| i + 2 + p);
        let mut k = fin_nombre;
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        if antes_es_espacio && fin_nombre > i + 2 && b.get(k) == Some(&b'=') {
            encontrados.push(texto[i..k + 1].to_string());
        }
    }
    encontrados
}

/// Revisa un archivo de la interfaz según su extensión.
fn escanear(archivo: &str, texto: &str) -> Vec<Hallazgo> {
    let mut hallazgos = Vec::new();
    let bajo = texto.to_ascii_lowercase();

    // En todos: ni `javascript:` ni recursos de otros orígenes.
    if bajo.contains("javascript:") {
        hallazgos.push(hallazgo("sin_javascript_url", "javascript:"));
    }
    let sin_espacio_svg = bajo.replace(&ESPACIO_SVG.to_ascii_lowercase(), "");
    for esquema in ["http://", "https://"] {
        for i in todas(&sin_espacio_svg, esquema) {
            let fin = (i + 40).min(sin_espacio_svg.len());
            hallazgos.push(hallazgo(
                "sin_recursos_externos",
                sin_espacio_svg.get(i..fin).unwrap_or(esquema),
            ));
        }
    }

    match archivo.rsplit('.').next().unwrap_or_default() {
        "js" => {
            for (regla, aguja) in PROHIBIDO_EN_JS {
                if texto.contains(aguja) {
                    hallazgos.push(hallazgo(regla, aguja));
                }
            }
        }
        "html" => {
            for i in todas(&bajo, "<script") {
                let fin = bajo[i..].find('>').map_or(bajo.len(), |p| i + p);
                let etiqueta = &bajo[i..fin];
                let vacia = bajo[fin..].starts_with("></script>");
                if !etiqueta.contains("src=\"/") || !vacia {
                    hallazgos.push(hallazgo("sin_scripts_en_linea", etiqueta));
                }
            }
            for aguja in ["<style", " style="] {
                if bajo.contains(aguja) {
                    hallazgos.push(hallazgo("sin_estilos_en_linea", aguja));
                }
            }
            for aguja in ["<iframe", "<object", "<embed", "<base", "<frame"] {
                if bajo.contains(aguja) {
                    hallazgos.push(hallazgo("sin_marcos_ni_objetos", aguja));
                }
            }
            for manejador in manejadores_en_linea(texto) {
                hallazgos.push(hallazgo("sin_manejadores_en_linea", manejador));
            }
            for atributo in ["src=\"", "href=\"", "action=\""] {
                for i in todas(&bajo, atributo) {
                    let valor = &bajo[i + atributo.len()..];
                    let propio = (valor.starts_with('/') && !valor.starts_with("//"))
                        || valor.starts_with('#');
                    if !propio {
                        let fin = valor.find('"').unwrap_or(valor.len());
                        hallazgos.push(hallazgo("sin_recursos_externos", &valor[..fin]));
                    }
                }
            }
        }
        "css" => {
            for aguja in [
                "@import",
                "url(",
                "expression(",
                "behavior:",
                "-moz-binding",
            ] {
                if bajo.contains(aguja) {
                    hallazgos.push(hallazgo("sin_recursos_ni_expresiones_en_css", aguja));
                }
            }
        }
        "svg" => {
            for aguja in ["<script", "<foreignobject", "href=", " style="] {
                if bajo.contains(aguja) {
                    hallazgos.push(hallazgo("svg_inerte", aguja));
                }
            }
            for manejador in manejadores_en_linea(texto) {
                hallazgos.push(hallazgo("svg_inerte", manejador));
            }
        }
        otra => hallazgos.push(hallazgo("tipo_desconocido", otra)),
    }
    hallazgos
}

fn texto_de(contenido: &[u8]) -> &str {
    std::str::from_utf8(contenido).expect("la interfaz es UTF-8")
}

#[test]
fn ningun_archivo_de_la_interfaz_tiene_nada_prohibido() {
    for recurso in &RECURSOS {
        let hallazgos = escanear(recurso.archivo, texto_de(recurso.contenido));
        assert!(hallazgos.is_empty(), "{}: {hallazgos:#?}", recurso.archivo);
    }
}

#[test]
fn el_escaner_encuentra_cada_cosa_prohibida() {
    let casos: Vec<(&str, String, &str)> = vec![
        ("a.js", "lista.innerHTML = datos;".into(), "sin_html_crudo"),
        ("a.js", "nodo.outerHTML = x;".into(), "sin_html_crudo"),
        (
            "a.js",
            "nodo.insertAdjacentHTML('beforeend', x);".into(),
            "sin_html_crudo",
        ),
        (
            "a.js",
            "rango.createContextualFragment(x);".into(),
            "sin_html_crudo",
        ),
        (
            "a.js",
            "new DOMParser().parseFromString(x, 'text/html');".into(),
            "sin_html_crudo",
        ),
        ("a.js", "marco.srcdoc = x;".into(), "sin_html_crudo"),
        ("a.js", "document.write(x);".into(), "sin_document_write"),
        ("a.js", "eval(codigo);".into(), "sin_eval"),
        ("a.js", "new Function('return 1')();".into(), "sin_eval"),
        ("a.js", "setTimeout(\"alerta()\", 10);".into(), "sin_eval"),
        ("a.js", "setTimeout('alerta()', 10);".into(), "sin_eval"),
        ("a.js", "setTimeout(`alerta()`, 10);".into(), "sin_eval"),
        ("a.js", "setInterval(\"alerta()\", 10);".into(), "sin_eval"),
        ("a.js", "setInterval('alerta()', 10);".into(), "sin_eval"),
        ("a.js", "setInterval(`alerta()`, 10);".into(), "sin_eval"),
        (
            "a.js",
            "nodo.style.color = 'red';".into(),
            "sin_estilos_en_linea",
        ),
        ("a.js", "nodo.style = x;".into(), "sin_estilos_en_linea"),
        ("a.js", "nodo.cssText = x;".into(), "sin_estilos_en_linea"),
        (
            "a.js",
            "nodo.setAttribute(\"style\", x);".into(),
            "sin_estilos_en_linea",
        ),
        (
            "a.js",
            "nodo.setAttribute('style', x);".into(),
            "sin_estilos_en_linea",
        ),
        (
            "a.js",
            "nodo.setAttribute('onclick', x);".into(),
            "sin_manejadores_en_linea",
        ),
        (
            "a.js",
            "nodo.setAttribute(\"onerror\", x);".into(),
            "sin_manejadores_en_linea",
        ),
        (
            "a.js",
            "import(\"/otro.js\");".into(),
            "sin_import_dinamico",
        ),
        (
            "a.js",
            "fetch(\"https://otro.example/datos\");".into(),
            "sin_recursos_externos",
        ),
        (
            "a.js",
            "location = \"javascript:alert(1)\";".into(),
            "sin_javascript_url",
        ),
        (
            "a.html",
            "<script>alert(1)</script>".into(),
            "sin_scripts_en_linea",
        ),
        (
            "a.html",
            "<script src=\"/a.js\">alert(1)</script>".into(),
            "sin_scripts_en_linea",
        ),
        (
            "a.html",
            "<script src=\"https://cdn.example/x.js\"></script>".into(),
            "sin_scripts_en_linea",
        ),
        (
            "a.html",
            "<style>p{}</style>".into(),
            "sin_estilos_en_linea",
        ),
        (
            "a.html",
            "<p style=\"color:red\">x</p>".into(),
            "sin_estilos_en_linea",
        ),
        (
            "a.html",
            "<img src=\"/x.png\" onerror=\"alerta()\">".into(),
            "sin_manejadores_en_linea",
        ),
        (
            "a.html",
            "<body onload = \"x()\">".into(),
            "sin_manejadores_en_linea",
        ),
        (
            "a.html",
            "<a href=\"https://otro.example/\">x</a>".into(),
            "sin_recursos_externos",
        ),
        (
            "a.html",
            "<link rel=\"stylesheet\" href=\"//cdn.example/x.css\">".into(),
            "sin_recursos_externos",
        ),
        (
            "a.html",
            "<iframe src=\"/x\"></iframe>".into(),
            "sin_marcos_ni_objetos",
        ),
        (
            "a.html",
            "<base href=\"/\">".into(),
            "sin_marcos_ni_objetos",
        ),
        (
            "a.html",
            "<a href=\"javascript:x()\">x</a>".into(),
            "sin_javascript_url",
        ),
        (
            "a.css",
            "@import \"otro.css\";".into(),
            "sin_recursos_ni_expresiones_en_css",
        ),
        (
            "a.css",
            "@font-face { src: url(/letra.woff2); }".into(),
            "sin_recursos_ni_expresiones_en_css",
        ),
        (
            "a.css",
            "p { width: expression(1); }".into(),
            "sin_recursos_ni_expresiones_en_css",
        ),
        (
            "a.svg",
            "<svg><script>alert(1)</script></svg>".into(),
            "svg_inerte",
        ),
        (
            "a.svg",
            "<svg><rect onclick=\"x()\"/></svg>".into(),
            "svg_inerte",
        ),
        (
            "a.svg",
            "<svg><image href=\"https://otro.example/x.png\"/></svg>".into(),
            "svg_inerte",
        ),
        (
            "a.svg",
            "<svg><foreignObject></foreignObject></svg>".into(),
            "svg_inerte",
        ),
        ("a.txt", "texto".into(), "tipo_desconocido"),
    ];
    for (archivo, texto, regla) in &casos {
        let hallazgos = escanear(archivo, texto);
        assert!(
            hallazgos.iter().any(|h| h.regla == *regla),
            "{archivo}: {texto:?} debía dar {regla}, dio {hallazgos:?}"
        );
    }
    let reglas: BTreeSet<&str> = casos.iter().map(|(_, _, regla)| *regla).collect();
    let en_js: BTreeSet<&str> = PROHIBIDO_EN_JS.iter().map(|(regla, _)| *regla).collect();
    assert!(en_js.is_subset(&reglas), "cada regla de JS tiene su caso");
    for (_, aguja) in PROHIBIDO_EN_JS {
        assert!(
            casos.iter().any(|(_, texto, _)| texto.contains(aguja)),
            "falta un caso para {aguja:?}"
        );
    }
}

#[test]
fn el_escaner_no_se_queja_de_lo_correcto() {
    for (archivo, texto) in [
        ("a.js", "const li = document.createElement(\"li\"); li.textContent = dato; lista.append(li);"),
        ("a.js", "setTimeout(() => nodo.remove(), 5000); nodo.dataset.tono = \"bien\";"),
        ("a.js", "await fetch(\"/api/estado\", { headers: { \"X-Consola\": \"1\" } });"),
        ("a.html", "<script type=\"module\" src=\"/app.js\"></script><a href=\"#grupo\">Grupo</a>"),
        ("a.html", "<link rel=\"stylesheet\" href=\"/app.css\"><button type=\"button\">Bien</button>"),
        ("a.html", "<p>Las condiciones online se cumplen, son = iguales</p>"),
        ("a.css", ":root { --fondo: #fff; } @media (prefers-color-scheme: dark) { :root { --fondo: #000; } }"),
        ("a.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\"/></svg>"),
    ] {
        assert_eq!(escanear(archivo, texto), [], "{archivo}: {texto}");
    }
}

#[test]
fn todo_lo_que_hay_en_web_esta_incrustado_y_escaneado() {
    let carpeta = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    let en_disco: BTreeSet<String> = std::fs::read_dir(&carpeta)
        .expect("leer web/")
        .map(|e| {
            e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let incrustados: BTreeSet<String> = RECURSOS.iter().map(|r| r.archivo.to_string()).collect();
    assert_eq!(en_disco, incrustados);
    for recurso in &RECURSOS {
        let disco = std::fs::read(carpeta.join(recurso.archivo)).expect("leer");
        assert_eq!(
            disco, recurso.contenido,
            "{} igual al incrustado",
            recurso.archivo
        );
    }
}

#[test]
fn la_interfaz_tiene_lo_minimo_de_accesibilidad_y_los_dos_temas() {
    let html = texto_de(RECURSOS[0].contenido);
    let css = texto_de(RECURSOS[1].contenido);
    assert!(html.contains("<html lang=\"es\">"));
    assert!(html.contains("name=\"viewport\""));
    assert!(
        html.contains("class=\"saltar\" href=\"#contenido\""),
        "enlace para saltar al contenido"
    );
    for seccion in ["grupo", "servicios", "sistema", "registros"] {
        assert!(
            html.contains(&format!("<section id=\"{seccion}\"")),
            "{seccion}"
        );
        assert!(
            html.contains(&format!("href=\"#{seccion}\"")),
            "{seccion} en la navegación"
        );
    }
    // Cada campo con su etiqueta.
    for (i, _) in html
        .match_indices("<input id=\"")
        .chain(html.match_indices("<select id=\""))
    {
        let resto = &html[i..];
        let id_inicio = resto.find("id=\"").expect("id") + 4;
        let id = &resto[id_inicio..id_inicio + resto[id_inicio..].find('"').expect("cierre")];
        assert!(
            html.contains(&format!("<label for=\"{id}\"")),
            "falta la etiqueta de {id}"
        );
    }
    assert!(
        css.contains("@media (prefers-color-scheme: dark)"),
        "tema oscuro"
    );
    assert!(css.contains(":focus-visible"), "foco visible");
    assert!(
        css.contains("reduced-motion"),
        "respeta a quien no quiere animaciones"
    );
}
