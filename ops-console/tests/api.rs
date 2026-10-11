//! La API de la consola: forma de cada respuesta, validación de los cuerpos, la cola del
//! registro redactada, el sistema sin valores y las cabeceras de seguridad en cada ruta.

mod comun;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use comun::{crudo, get, post, Carpeta, Procesos};
use ops_console::api::{Consola, Piezas, LINEAS_MAX};
use ops_console::direccion::DireccionLocal;
use ops_console::estaticos::RECURSOS;
use ops_console::grupo::{Grupo, Politica};
use ops_console::http::{self, Cabeza, Limites, Metodo, Peticion, CABECERAS_DE_SEGURIDAD};
use ops_console::json::{self, Json};
use ops_console::registros::MARCA;
use ops_console::reloj::RelojDeMentira;
use ops_console::servidor::{Manejador, Servidor};
use ops_console::sistema::{EntornoFijo, VARIABLES};
use ops_console::vigia::{EstadoServicio, VigiaFijo};

const REGISTRO: &str = "INFO arranque\nWARN token=abc123 vencido\nERROR para ana@example.com\n";

struct Banco {
    consola: Arc<Consola>,
    procesos: Procesos,
    reloj: Arc<RelojDeMentira>,
    carpeta: Carpeta,
}

fn banco_con(registro: Option<&str>, entorno: EntornoFijo) -> Banco {
    let carpeta = Carpeta::nueva("api");
    let registros = match registro {
        Some(contenido) => carpeta.archivo("servicio.log", contenido.as_bytes()),
        None => carpeta.ruta().join("no-existe.log"),
    };
    let procesos = Procesos::default();
    let reloj = Arc::new(RelojDeMentira::nuevo());
    let grupo = Grupo::nuevo(
        Politica::nueva(2, 5, Duration::from_secs(60)).expect("política"),
        procesos.fabrica(),
        reloj.clone(),
    )
    .expect("grupo");
    let vigia = VigiaFijo::default()
        .con(
            "miniaturas-api.service",
            EstadoServicio::Activo {
                subestado: "running".into(),
                pid: Some(4242),
                reinicios: Some(0),
            },
        )
        .con(
            "miniaturas-cola.service",
            EstadoServicio::Fallido {
                subestado: "failed".into(),
            },
        );
    let consola = Arc::new(Consola::nueva(Piezas {
        grupo,
        vigia: Box::new(vigia),
        servicios: vec![
            "miniaturas-api.service".into(),
            "miniaturas-cola.service".into(),
            "nada.service".into(),
        ],
        registros,
        entorno: Box::new(entorno),
        reloj: reloj.clone(),
    }));
    Banco {
        consola,
        procesos,
        reloj,
        carpeta,
    }
}

fn banco() -> Banco {
    banco_con(Some(REGISTRO), EntornoFijo::default())
}

/// Una petición que ya pasó las defensas del servidor, directo a la consola.
fn pedir(c: &Consola, metodo: Metodo, objetivo: &str, cuerpo: &str) -> http::Respuesta {
    let (ruta, consulta) = match objetivo.split_once('?') {
        Some((r, q)) => (r.to_string(), Some(q.to_string())),
        None => (objetivo.to_string(), None),
    };
    c.atender(&Peticion {
        cabeza: Cabeza {
            metodo,
            ruta,
            consulta,
            cabeceras: Vec::new(),
            longitud: Some(cuerpo.len()),
        },
        cuerpo: cuerpo.as_bytes().to_vec(),
    })
}

fn leer(r: &http::Respuesta) -> Json {
    json::analizar(std::str::from_utf8(&r.cuerpo).expect("UTF-8")).expect("JSON")
}

fn datos(r: &http::Respuesta) -> Json {
    assert_eq!(r.estado, 200, "{}", String::from_utf8_lossy(&r.cuerpo));
    let j = leer(r);
    assert_eq!(j.campo("ok"), Some(&Json::Bool(true)));
    j.campo("datos").cloned().expect("datos")
}

fn error(r: &http::Respuesta) -> (u16, String) {
    let j = leer(r);
    assert_eq!(j.campo("ok"), Some(&Json::Bool(false)));
    match j.campo("error").and_then(|e| e.campo("codigo")) {
        Some(Json::Texto(codigo)) => (r.estado, codigo.clone()),
        otro => panic!("sin código: {otro:?}"),
    }
}

fn entero(j: &Json, clave: &str) -> i64 {
    j.campo(clave)
        .and_then(Json::como_entero)
        .unwrap_or_else(|| panic!("«{clave}» no es un entero en {}", j.a_texto()))
}

fn texto(j: &Json, clave: &str) -> String {
    match j.campo(clave) {
        Some(Json::Texto(t)) => t.clone(),
        otro => panic!("«{clave}» no es texto: {otro:?}"),
    }
}

fn lista(j: &Json, clave: &str) -> Vec<Json> {
    match j.campo(clave) {
        Some(Json::Lista(l)) => l.clone(),
        otro => panic!("«{clave}» no es una lista: {otro:?}"),
    }
}

fn estado(c: &Consola) -> Json {
    datos(&pedir(c, Metodo::Get, "/api/estado", ""))
}

fn ids(c: &Consola) -> Vec<i64> {
    lista(&estado(c), "posiciones")
        .iter()
        .map(|p| entero(p, "id"))
        .collect()
}

// ─── Grupo ────────────────────────────────────────────────────────────────────────────────

#[test]
fn el_estado_muestra_la_consola_la_politica_y_las_posiciones() {
    let b = banco();
    let e = estado(&b.consola);
    assert_eq!(
        e.claves(),
        ["consola", "politica", "posiciones", "ultimo_tick"]
    );
    assert_eq!(
        e.campo("politica"),
        Some(&Json::objeto([
            ("min", 2u32.into()),
            ("max", 5u32.into()),
            ("vida_s", 60u32.into()),
            ("tope", 16u32.into()),
        ]))
    );
    let posiciones = lista(&e, "posiciones");
    assert_eq!(posiciones.len(), 2);
    assert_eq!(texto(&posiciones[0], "estado"), "viva");
    assert_eq!(e.campo("ultimo_tick"), Some(&Json::Nulo));
    let consola = e.campo("consola").expect("consola");
    assert_eq!(consola.claves(), ["version", "plataforma", "activa_ms"]);
}

#[test]
fn reciclar_por_la_api_conserva_la_posicion() {
    let b = banco();
    let pid_antes = entero(&lista(&estado(&b.consola), "posiciones")[1], "pid");
    let r = datos(&pedir(
        &b.consola,
        Metodo::Post,
        "/api/grupo/reciclar",
        r#"{"posicion": 2}"#,
    ));
    assert_eq!(entero(&r, "posicion"), 2);
    assert_eq!(texto(&r, "motivo"), "manual");
    assert_eq!(entero(&r, "pid_anterior"), pid_antes);
    assert_eq!(r.campo("error"), Some(&Json::Nulo));
    let nuevo = entero(&r, "pid_nuevo");

    assert_eq!(ids(&b.consola), [1, 2]);
    let posicion = &lista(&estado(&b.consola), "posiciones")[1];
    assert_eq!(entero(posicion, "pid"), nuevo);
    assert_eq!(texto(posicion, "estado"), "reciclada");
}

#[test]
fn reciclar_rechaza_entero_lo_que_no_entiende() {
    let b = banco();
    for (cuerpo, estado, codigo) in [
        (r#"{"posicion": 99}"#, 404, "posicion_desconocida"),
        (r#"{}"#, 400, "falta_campo"),
        (r#"{"posicion": "2"}"#, 400, "tipo_invalido"),
        (r#"{"posicion": -1}"#, 400, "valor_invalido"),
        (r#"{"posicion": 2, "otra": 1}"#, 400, "campo_desconocido"),
        (r#"{"posicion": 2.5}"#, 400, "json_invalido"),
        (r#"{"posicion": 1, "posicion": 2}"#, 400, "json_invalido"),
        (r#"[2]"#, 400, "json_invalido"),
        ("no es JSON", 400, "json_invalido"),
        ("", 400, "json_invalido"),
    ] {
        let r = pedir(&b.consola, Metodo::Post, "/api/grupo/reciclar", cuerpo);
        assert_eq!(error(&r), (estado, codigo.to_string()), "{cuerpo:?}");
    }
    assert!(b.procesos.terminados().is_empty(), "nada se recicló");
}

#[test]
fn escalar_por_la_api_respeta_el_minimo_y_el_maximo() {
    let b = banco();
    for (cuerpo, codigo) in [
        (r#"{"cantidad": 1}"#, "fuera_de_politica"),
        (r#"{"cantidad": 0}"#, "fuera_de_politica"),
        (r#"{"cantidad": 6}"#, "fuera_de_politica"),
        (r#"{"cantidad": -3}"#, "valor_invalido"),
        (r#"{"cantidad": "4"}"#, "tipo_invalido"),
    ] {
        let r = pedir(&b.consola, Metodo::Post, "/api/grupo/escala", cuerpo);
        assert_eq!(error(&r), (400, codigo.to_string()), "{cuerpo}");
        assert_eq!(ids(&b.consola).len(), 2, "{cuerpo}: el grupo no cambió");
    }
    let r = datos(&pedir(
        &b.consola,
        Metodo::Post,
        "/api/grupo/escala",
        r#"{"cantidad": 4}"#,
    ));
    assert_eq!((entero(&r, "antes"), entero(&r, "despues")), (2, 4));
    assert_eq!(ids(&b.consola), [1, 2, 3, 4]);
}

#[test]
fn una_politica_invalida_es_400_y_no_cambia_nada() {
    let b = banco();
    for (cuerpo, codigo) in [
        (r#"{"min": 3, "max": 2}"#, "politica_invalida"),
        (r#"{"min": 0, "max": 2}"#, "politica_invalida"),
        (r#"{"min": 1, "max": 17}"#, "politica_invalida"),
        (r#"{"min": 2}"#, "politica_incompleta"),
        (r#"{"max": 2}"#, "politica_incompleta"),
        (r#"{}"#, "falta_campo"),
        // Política y cantidad juntas: o se aplica todo o nada.
        (
            r#"{"min": 3, "max": 4, "cantidad": 5}"#,
            "fuera_de_politica",
        ),
    ] {
        let r = pedir(&b.consola, Metodo::Post, "/api/grupo/escala", cuerpo);
        assert_eq!(error(&r), (400, codigo.to_string()), "{cuerpo}");
        let politica = estado(&b.consola)
            .campo("politica")
            .cloned()
            .expect("política");
        assert_eq!(
            (entero(&politica, "min"), entero(&politica, "max")),
            (2, 5),
            "{cuerpo}"
        );
        assert_eq!(ids(&b.consola).len(), 2);
    }
    let r = datos(&pedir(
        &b.consola,
        Metodo::Post,
        "/api/grupo/escala",
        r#"{"min": 3, "max": 6, "cantidad": 6}"#,
    ));
    assert_eq!((entero(&r, "antes"), entero(&r, "despues")), (2, 6));
    let politica = r.campo("politica").expect("política");
    assert_eq!((entero(politica, "min"), entero(politica, "max")), (3, 6));
}

#[test]
fn el_tick_reemplaza_lo_que_murio_y_queda_anotado() {
    let b = banco();
    let caido = entero(&lista(&estado(&b.consola), "posiciones")[0], "pid");
    b.procesos.matar(u32::try_from(caido).expect("pid"));

    let r = datos(&pedir(&b.consola, Metodo::Post, "/api/grupo/tick", "{}"));
    let acciones = lista(&r, "acciones");
    assert_eq!(acciones.len(), 1);
    assert_eq!(entero(&acciones[0], "posicion"), 1);
    assert_eq!(texto(&acciones[0], "motivo"), "murio");
    assert_eq!(entero(&acciones[0], "pid_anterior"), caido);

    b.reloj.avanzar(Duration::from_secs(3));
    let ultimo = estado(&b.consola)
        .campo("ultimo_tick")
        .cloned()
        .expect("último tick");
    assert_eq!(texto(&ultimo, "origen"), "manual");
    assert_eq!(entero(&ultimo, "hace_ms"), 3000);
    assert_eq!(lista(&ultimo, "acciones"), acciones);

    assert!(b.consola.tick().is_empty());
    let ultimo = estado(&b.consola)
        .campo("ultimo_tick")
        .cloned()
        .expect("último tick");
    assert_eq!(texto(&ultimo, "origen"), "automatico");

    let r = pedir(
        &b.consola,
        Metodo::Post,
        "/api/grupo/tick",
        r#"{"forzar": true}"#,
    );
    assert_eq!(error(&r), (400, "campo_desconocido".into()));
}

// ─── Servicios ────────────────────────────────────────────────────────────────────────────

#[test]
fn los_servicios_salen_del_vigia_y_se_recuerdan_diez_segundos() {
    let b = banco();
    let r = datos(&pedir(&b.consola, Metodo::Get, "/api/servicios", ""));
    assert_eq!((entero(&r, "limite_s"), entero(&r, "cache_s")), (3, 10));
    let servicios = lista(&r, "servicios");
    let resumen: Vec<(String, String)> = servicios
        .iter()
        .map(|s| (texto(s, "unidad"), texto(s, "estado")))
        .collect();
    assert_eq!(
        resumen,
        [
            ("miniaturas-api.service".into(), "activo".into()),
            ("miniaturas-cola.service".into(), "fallido".into()),
            ("nada.service".into(), "no_encontrado".into()),
        ]
    );
    assert_eq!(
        texto(&servicios[0], "detalle"),
        "running · pid 4242 · 0 reinicios"
    );
    assert_eq!(servicios[0].campo("de_cache"), Some(&Json::Bool(false)));

    b.reloj.avanzar(Duration::from_secs(4));
    let r = datos(&pedir(&b.consola, Metodo::Get, "/api/servicios", ""));
    let primero = &lista(&r, "servicios")[0];
    assert_eq!(primero.campo("de_cache"), Some(&Json::Bool(true)));
    assert_eq!(entero(primero, "hace_ms"), 4000);
}

// ─── Registros ────────────────────────────────────────────────────────────────────────────

fn lineas(r: &Json) -> Vec<String> {
    lista(r, "lineas")
        .iter()
        .map(|l| match l {
            Json::Texto(t) => t.clone(),
            otro => panic!("línea que no es texto: {otro:?}"),
        })
        .collect()
}

#[test]
fn los_registros_salen_redactados() {
    let b = banco();
    let r = datos(&pedir(&b.consola, Metodo::Get, "/api/registros", ""));
    assert_eq!(
        lineas(&r),
        [
            "INFO arranque".to_string(),
            format!("WARN token={MARCA} vencido"),
            format!("ERROR para {MARCA}"),
        ]
    );
    assert_eq!(entero(&r, "redacciones"), 2);
    assert_eq!(
        texto(&r, "archivo"),
        "servicio.log",
        "solo el nombre, nunca la ruta"
    );
    assert_eq!(r.campo("recortado"), Some(&Json::Bool(false)));
    assert_eq!(entero(&r, "tope"), 256 * 1024);

    let r = datos(&pedir(&b.consola, Metodo::Get, "/api/registros?n=1", ""));
    assert_eq!(lineas(&r), [format!("ERROR para {MARCA}")]);
    let maximo = format!("/api/registros?n={LINEAS_MAX}");
    assert_eq!(pedir(&b.consola, Metodo::Get, &maximo, "").estado, 200);
}

#[test]
fn un_n_fuera_de_rango_es_400() {
    let b = banco();
    let demasiado = format!("{}", LINEAS_MAX + 1);
    for n in [
        "0",
        "-1",
        "abc",
        "",
        "1.5",
        demasiado.as_str(),
        "99999999999999999999999",
    ] {
        let r = pedir(
            &b.consola,
            Metodo::Get,
            &format!("/api/registros?n={n}"),
            "",
        );
        assert_eq!(error(&r), (400, "parametro_invalido".into()), "n={n:?}");
    }
}

#[test]
fn el_texto_del_registro_no_puede_inyectar_marcado() {
    let malicioso = "<script>alert(1)</script></li><img src=x onerror=alert(2)> & \u{2028}fin\n";
    let b = banco_con(Some(malicioso), EntornoFijo::default());
    let r = pedir(&b.consola, Metodo::Get, "/api/registros", "");
    let crudo = String::from_utf8(r.cuerpo.clone()).expect("UTF-8");
    for caracter in ['<', '>', '&', '\u{2028}'] {
        assert!(
            !crudo.contains(caracter),
            "{caracter:?} sin escapar en {crudo}"
        );
    }
    assert_eq!(
        lineas(&datos(&r)),
        [malicioso.trim_end().to_string()],
        "el texto llega intacto"
    );
}

#[test]
fn un_registro_que_no_se_puede_leer_es_503_y_no_revela_la_ruta() {
    let b = banco_con(None, EntornoFijo::default());
    let r = pedir(&b.consola, Metodo::Get, "/api/registros", "");
    assert_eq!(error(&r), (503, "registro_no_disponible".into()));
    let cuerpo = String::from_utf8_lossy(&r.cuerpo).into_owned();
    assert!(cuerpo.contains("el archivo no existe"), "{cuerpo}");
    let carpeta = b.carpeta.ruta().to_string_lossy().into_owned();
    assert!(!cuerpo.contains(&carpeta), "{cuerpo}");
    assert!(!cuerpo.contains("no-existe.log"), "{cuerpo}");
}

// ─── Sistema ──────────────────────────────────────────────────────────────────────────────

fn variables(c: &Consola) -> (Json, String) {
    let r = pedir(c, Metodo::Get, "/api/sistema", "");
    let cuerpo = String::from_utf8_lossy(&r.cuerpo).into_owned();
    let d = datos(&r);
    assert_eq!(
        d.claves(),
        ["variables"],
        "el sistema solo informa las variables"
    );
    (d.campo("variables").cloned().expect("variables"), cuerpo)
}

#[test]
fn el_sistema_dice_si_cada_variable_esta_definida_y_nunca_su_valor() {
    let secreto = |nombre: &str| format!("valor-secreto-de-{}", nombre.to_lowercase());

    // Una por una: solo la definida dice que sí, y su valor no aparece.
    for definida in VARIABLES {
        let b = banco_con(
            Some(REGISTRO),
            EntornoFijo::default().con(definida, &secreto(definida)),
        );
        let (vars, cuerpo) = variables(&b.consola);
        assert_eq!(
            vars.claves(),
            VARIABLES,
            "todas las claves, en orden, ni una más"
        );
        for nombre in VARIABLES {
            assert_eq!(
                vars.campo(nombre),
                Some(&Json::Bool(nombre == definida)),
                "{nombre}"
            );
        }
        assert!(!cuerpo.contains("valor-secreto"), "{cuerpo}");
    }

    // Todas definidas, y ninguna.
    let todas = VARIABLES.iter().fold(EntornoFijo::default(), |e, nombre| {
        e.con(nombre, &secreto(nombre))
    });
    let b = banco_con(Some(REGISTRO), todas);
    let (vars, cuerpo) = variables(&b.consola);
    assert!(VARIABLES
        .iter()
        .all(|n| vars.campo(n) == Some(&Json::Bool(true))));
    assert!(!cuerpo.contains("valor-secreto"), "{cuerpo}");

    let b = banco();
    let (vars, _) = variables(&b.consola);
    assert!(VARIABLES
        .iter()
        .all(|n| vars.campo(n) == Some(&Json::Bool(false))));
}

// ─── Rutas, métodos y forma de las respuestas ─────────────────────────────────────────────

#[test]
fn cada_ruta_responde_solo_a_su_metodo() {
    let b = banco();
    for ruta in [
        "/api/estado",
        "/api/servicios",
        "/api/registros",
        "/api/sistema",
        "/",
    ] {
        let r = pedir(&b.consola, Metodo::Post, ruta, "{}");
        assert_eq!(error(&r), (405, "metodo_no_permitido".into()), "{ruta}");
        assert!(r.extra.contains(&("Allow", "GET".into())), "{ruta}");
    }
    for ruta in [
        "/api/grupo/reciclar",
        "/api/grupo/escala",
        "/api/grupo/tick",
    ] {
        let r = pedir(&b.consola, Metodo::Get, ruta, "");
        assert_eq!(error(&r), (405, "metodo_no_permitido".into()), "{ruta}");
        assert!(r.extra.contains(&("Allow", "POST".into())), "{ruta}");
    }
    for ruta in [
        "/nada",
        "/index.html",
        "/web/app.js",
        "/../Cargo.toml",
        "/api",
        "/api/",
    ] {
        let r = pedir(&b.consola, Metodo::Get, ruta, "");
        assert_eq!(error(&r), (404, "ruta_desconocida".into()), "{ruta}");
    }
}

#[test]
fn la_interfaz_se_sirve_incrustada_con_su_tipo() {
    let b = banco();
    for recurso in &RECURSOS {
        let r = pedir(&b.consola, Metodo::Get, recurso.ruta, "");
        assert_eq!(r.estado, 200, "{}", recurso.ruta);
        assert_eq!(r.tipo, recurso.tipo);
        assert_eq!(r.cuerpo, recurso.contenido);
    }
    let tipos: Vec<&str> = RECURSOS.iter().map(|r| r.tipo).collect();
    assert_eq!(
        tipos,
        [
            "text/html; charset=utf-8",
            "text/css; charset=utf-8",
            "text/javascript; charset=utf-8",
            "image/svg+xml",
        ]
    );
}

/// Un servidor real con la consola, para ver la respuesta entera, cabeceras incluidas.
fn en_marcha(b: &Banco) -> Servidor {
    let direccion = DireccionLocal::desde_texto("127.0.0.1:0").expect("dirección");
    Servidor::escuchar(direccion, Limites::default(), b.consola.clone()).expect("escuchar")
}

/// Todas las respuestas que puede dar la consola, buenas y malas, por la red.
fn todas_las_respuestas(d: std::net::SocketAddr) -> Vec<(String, comun::Respuesta)> {
    let mut respuestas = Vec::new();
    for ruta in [
        "/",
        "/app.css",
        "/app.js",
        "/icono.svg",
        "/api/estado",
        "/api/servicios",
        "/api/registros",
        "/api/registros?n=2",
        "/api/registros?n=0",
        "/api/sistema",
        "/api/grupo/tick",
        "/no-existe",
    ] {
        respuestas.push((format!("GET {ruta}"), get(d, ruta)));
    }
    for (ruta, cuerpo) in [
        ("/api/grupo/reciclar", r#"{"posicion": 1}"#),
        ("/api/grupo/reciclar", r#"{"posicion": 99}"#),
        ("/api/grupo/escala", r#"{"cantidad": 3}"#),
        ("/api/grupo/escala", r#"{"cantidad": 1}"#),
        ("/api/grupo/tick", "{}"),
        ("/api/estado", "{}"),
        ("/", "{}"),
    ] {
        respuestas.push((format!("POST {ruta} {cuerpo}"), post(d, ruta, cuerpo)));
    }
    let port = d.port();
    for (nombre, peticion) in [
        ("host ajeno", format!("GET / HTTP/1.1\r\nHost: atacante.example:{port}\r\n\r\n")),
        (
            "post sin json",
            format!("POST /api/grupo/tick HTTP/1.1\r\nHost: {d}\r\nX-Consola: 1\r\nContent-Length: 2\r\n\r\n{{}}"),
        ),
        (
            "post sin cabecera propia",
            format!("POST /api/grupo/tick HTTP/1.1\r\nHost: {d}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"),
        ),
        (
            "post de otro origen",
            format!("POST /api/grupo/tick HTTP/1.1\r\nHost: {d}\r\nContent-Type: application/json\r\nX-Consola: 1\r\nOrigin: http://atacante.example\r\nContent-Length: 2\r\n\r\n{{}}"),
        ),
        (
            "cuerpo grande",
            format!("POST /api/grupo/tick HTTP/1.1\r\nHost: {d}\r\nContent-Type: application/json\r\nX-Consola: 1\r\nContent-Length: 999999\r\n\r\n"),
        ),
        ("metodo", format!("DELETE / HTTP/1.1\r\nHost: {d}\r\n\r\n")),
        ("basura", "HOLA\r\n\r\n".to_string()),
    ] {
        respuestas.push((nombre.to_string(), crudo(d, peticion.as_bytes())));
    }
    respuestas
}

#[test]
fn cada_ruta_lleva_las_cabeceras_de_seguridad_y_ninguna_de_cors() {
    let b = banco();
    let s = en_marcha(&b);
    let respuestas = todas_las_respuestas(s.direccion());
    assert!(respuestas.len() >= 25);
    for (que, r) in &respuestas {
        for (nombre, valor) in CABECERAS_DE_SEGURIDAD {
            let nombre = nombre.to_ascii_lowercase();
            assert_eq!(r.veces(&nombre), 1, "{que}: {nombre}");
            assert_eq!(r.cabecera(&nombre), Some(valor), "{que}: {nombre}");
        }
        let csp = r.cabecera("content-security-policy").unwrap_or_default();
        assert!(csp.starts_with("default-src 'self'"), "{que}: {csp}");
        assert!(
            !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
            "{que}: {csp}"
        );
        assert_eq!(
            r.cabecera("x-content-type-options"),
            Some("nosniff"),
            "{que}"
        );
        assert_eq!(r.cabecera("referrer-policy"), Some("no-referrer"), "{que}");
        assert_eq!(r.cabecera("x-frame-options"), Some("DENY"), "{que}");
        assert_eq!(r.cabecera("cache-control"), Some("no-store"), "{que}");
        assert_eq!(r.cabecera("connection"), Some("close"), "{que}");
        assert!(
            r.cabeceras
                .iter()
                .all(|(n, _)| !n.starts_with("access-control-")),
            "{que}: sin CORS"
        );
    }
}

#[test]
fn todos_los_errores_tienen_la_misma_forma() {
    let b = banco();
    let s = en_marcha(&b);
    let mut errores = 0;
    for (que, r) in todas_las_respuestas(s.direccion()) {
        if r.estado < 400 {
            continue;
        }
        errores += 1;
        assert!(
            r.cabecera("content-type")
                .is_some_and(|t| t.starts_with("application/json")),
            "{que}"
        );
        let j = r.json();
        assert_eq!(j.claves(), ["ok", "error"], "{que}");
        assert_eq!(j.campo("ok"), Some(&Json::Bool(false)), "{que}");
        let e = j.campo("error").expect("error");
        assert_eq!(e.claves(), ["estado", "codigo", "mensaje"], "{que}");
        assert_eq!(entero(e, "estado"), i64::from(r.estado), "{que}");
        assert!(
            !texto(e, "codigo").is_empty() && !texto(e, "mensaje").is_empty(),
            "{que}"
        );
    }
    assert!(errores >= 12, "se probaron {errores} errores");
}

#[test]
fn ninguna_respuesta_de_la_api_lleva_la_ruta_del_registro() {
    let b = banco();
    let s = en_marcha(&b);
    let carpeta: PathBuf = b.carpeta.ruta().to_path_buf();
    let carpeta = carpeta.to_string_lossy().into_owned();
    for (que, r) in todas_las_respuestas(s.direccion()) {
        assert!(!r.texto().contains(&carpeta), "{que}");
    }
}

// ─── JSON: escritura inerte y lectura estricta ───────────────────────────────────────────

#[test]
fn el_json_que_se_escribe_queda_inerte_dentro_de_un_html() {
    let texto = Json::from("<script>&\u{2028}\u{2029}\"\\\n\u{1}ñ");
    assert_eq!(
        texto.a_texto(),
        r#""\u003cscript\u003e\u0026\u2028\u2029\"\\\n\u0001ñ""#
    );
    let valor = Json::objeto([
        (
            "lista",
            vec![Json::Nulo, true.into(), (-7i64).into(), "x".into()].into(),
        ),
        ("vacio", Json::objeto([])),
    ]);
    assert_eq!(
        json::analizar(&valor.a_texto()),
        Ok(valor.clone()),
        "ida y vuelta"
    );
}

#[test]
fn el_json_ambiguo_o_incompleto_se_rechaza() {
    let profundo = format!("{}1{}", "[".repeat(9), "]".repeat(9));
    for texto in [
        "",
        " ",
        "-",
        "+1",
        "01",
        "1.5",
        "1e3",
        "9223372036854775808",
        "1 2",
        "tru",
        "nul",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\" 1}",
        "{a:1}",
        "{\"a\":1,\"a\":2}",
        "\"sin cerrar",
        "\"\\x\"",
        "\"\\u+041\"",
        "\"\\u12\"",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\ud800\\u0041\"",
        "\"control\u{1}crudo\"",
        profundo.as_str(),
    ] {
        assert!(
            json::analizar(texto).is_err(),
            "debía rechazarse: {texto:?}"
        );
    }
}

#[test]
fn el_json_valido_se_lee_entero() {
    let ocho = format!("{}1{}", "[".repeat(8), "]".repeat(8));
    assert!(json::analizar(&ocho).is_ok(), "ocho niveles sí");
    assert_eq!(json::analizar("-0"), Ok(Json::Entero(0)));
    assert_eq!(
        json::analizar("\"\\ud83d\\ude00 \\u00f1\\/\""),
        Ok(Json::from("😀 ñ/"))
    );
    assert_eq!(
        json::analizar(" {\"a\" : [1, true, null, \"x\"] } "),
        Ok(Json::objeto([(
            "a",
            vec![Json::Entero(1), Json::Bool(true), Json::Nulo, "x".into()].into()
        )]))
    );
}
