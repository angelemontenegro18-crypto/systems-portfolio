//! El rastreador con un transporte falso y un reloj controlado: cada regla de
//! cortesía se verifica de forma determinista, sin red ni esperas reales.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use polite_crawler::{Config, Identidad, MotivoEspera, Rastreador, Rechazo, Respuesta, Transporte};
use url::Url;

/// Transporte que responde lo que se le programa y anota cada petición.
#[derive(Default)]
struct Falso {
    respuestas: HashMap<String, Result<Respuesta, String>>,
    pedidas: RefCell<Vec<(String, String)>>,
}

impl Falso {
    fn con(mut self, url: &str, estado: u16, cuerpo: &str) -> Self {
        self.respuestas.insert(url.into(), Ok(Respuesta { estado, cuerpo: cuerpo.as_bytes().to_vec(), ..Default::default() }));
        self
    }
    fn con_respuesta(mut self, url: &str, r: Respuesta) -> Self {
        self.respuestas.insert(url.into(), Ok(r));
        self
    }
    fn con_falla(mut self, url: &str) -> Self {
        self.respuestas.insert(url.into(), Err("conexión rechazada".into()));
        self
    }
}

impl Transporte for &Falso {
    fn get(&self, url: &Url, user_agent: &str) -> Result<Respuesta, String> {
        self.pedidas.borrow_mut().push((url.to_string(), user_agent.to_string()));
        self.respuestas.get(url.as_str()).cloned().unwrap_or(Ok(Respuesta { estado: 404, ..Default::default() }))
    }
}

const SITIO: &str = "https://sitio.test";

fn identidad() -> Identidad {
    Identidad::nueva("Lector", "1.0", "https://lector.test/bot").expect("identidad válida")
}

fn config() -> Config {
    Config {
        intervalo_minimo: Duration::from_millis(1000),
        max_paginas_por_origen: 100,
        espera_inicial: Duration::from_millis(2000),
        espera_maxima: Duration::from_millis(16_000),
        vigencia_robots: Duration::from_secs(3600),
        reintento_robots: Duration::from_secs(60),
    }
}

fn rastreador(f: &Falso) -> Rastreador<&Falso> {
    Rastreador::nuevo(f, identidad(), config())
}

fn pedidas(f: &Falso) -> Vec<String> {
    f.pedidas.borrow().iter().map(|(u, _)| u.replace(SITIO, "")).collect()
}

fn esperar_hasta(r: Result<polite_crawler::Pagina, Rechazo>) -> (u64, MotivoEspera) {
    match r {
        Err(Rechazo::Esperar { desde_ms, motivo }) => (desde_ms, motivo),
        otro => panic!("se esperaba Esperar: {otro:?}"),
    }
}

// ─── robots.txt ──────────────────────────────────────────────────────────

#[test]
fn leer_robots_cuenta_como_peticion_y_la_pagina_llega_en_la_siguiente() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 200, "User-agent: *\nDisallow:\n").con(
        &format!("{SITIO}/a"),
        200,
        "hola",
    );
    let mut r = rastreador(&f);

    let (desde, motivo) = esperar_hasta(r.obtener(&format!("{SITIO}/a"), 0));
    assert_eq!((desde, motivo), (1000, MotivoEspera::Intervalo));
    assert_eq!(pedidas(&f), vec!["/robots.txt"], "todavía no se pidió la página");

    let p = r.obtener(&format!("{SITIO}/a"), 1000).expect("página");
    assert_eq!((p.estado, p.cuerpo.as_slice()), (200, &b"hola"[..]));
    assert_eq!(pedidas(&f), vec!["/robots.txt", "/a"]);
}

#[test]
fn una_ruta_prohibida_se_rechaza_con_su_regla_y_nunca_se_pide() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 200, "User-agent: lector\nDisallow: /privado\n");
    let mut r = rastreador(&f);
    match r.obtener(&format!("{SITIO}/privado/datos"), 0) {
        Err(Rechazo::ProhibidoPorRobots { motivo }) => assert_eq!(motivo, "Disallow: /privado"),
        otro => panic!("{otro:?}"),
    }
    // Aunque pase el tiempo, sigue prohibida y sigue sin pedirse.
    assert!(matches!(r.obtener(&format!("{SITIO}/privado/datos"), 50_000), Err(Rechazo::ProhibidoPorRobots { .. })));
    assert_eq!(pedidas(&f), vec!["/robots.txt"]);
}

#[test]
fn robots_con_4xx_significa_sin_reglas() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "").con(&format!("{SITIO}/a"), 200, "ok");
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/a"), 0);
    assert_eq!(r.obtener(&format!("{SITIO}/a"), 1000).map(|p| p.estado), Ok(200));
}

#[test]
fn robots_con_5xx_429_o_inalcanzable_prohibe_todo_y_se_reintenta_despues() {
    for (nombre, falso) in [
        ("503", Falso::default().con(&format!("{SITIO}/robots.txt"), 503, "")),
        ("429", Falso::default().con(&format!("{SITIO}/robots.txt"), 429, "")),
        ("sin red", Falso::default().con_falla(&format!("{SITIO}/robots.txt"))),
    ] {
        let mut r = rastreador(&falso);
        match r.obtener(&format!("{SITIO}/a"), 0) {
            Err(Rechazo::ProhibidoPorRobots { motivo }) => {
                assert!(motivo.contains("se asume todo prohibido"), "{nombre}: {motivo}")
            }
            otro => panic!("{nombre}: {otro:?}"),
        }
        // Dentro del plazo de reintento no se vuelve a pedir robots.txt…
        let _ = r.obtener(&format!("{SITIO}/a"), 30_000);
        assert_eq!(pedidas(&falso), vec!["/robots.txt"], "{nombre}");
        // …y después sí.
        let _ = r.obtener(&format!("{SITIO}/a"), 61_000);
        assert_eq!(pedidas(&falso), vec!["/robots.txt", "/robots.txt"], "{nombre}");
    }
}

#[test]
fn robots_se_vuelve_a_leer_al_vencer() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 200, "User-agent: *\nDisallow:\n");
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/a"), 0);
    let _ = r.obtener(&format!("{SITIO}/a"), 1_000);
    let _ = r.obtener(&format!("{SITIO}/a"), 3_600_000);
    assert_eq!(pedidas(&f).iter().filter(|p| *p == "/robots.txt").count(), 2);
}

#[test]
fn las_redirecciones_de_robots_se_siguen() {
    let f = Falso::default()
        .con_respuesta(
            &format!("{SITIO}/robots.txt"),
            Respuesta { estado: 301, location: Some("/reglas.txt".into()), ..Default::default() },
        )
        .con(&format!("{SITIO}/reglas.txt"), 200, "User-agent: *\nDisallow: /x\n");
    let mut r = rastreador(&f);
    assert!(matches!(r.obtener(&format!("{SITIO}/x"), 0), Err(Rechazo::ProhibidoPorRobots { .. })));
}

// ─── Intervalo, crawl-delay y presupuesto ────────────────────────────────

#[test]
fn el_intervalo_se_respeta_por_origen_y_los_origenes_son_independientes() {
    let otro = "https://otro.test";
    let f = Falso::default()
        .con(&format!("{SITIO}/robots.txt"), 404, "")
        .con(&format!("{otro}/robots.txt"), 404, "");
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/a"), 0);
    let _ = r.obtener(&format!("{otro}/a"), 0);
    assert!(r.obtener(&format!("{SITIO}/a"), 1000).is_ok());
    assert_eq!(esperar_hasta(r.obtener(&format!("{SITIO}/b"), 1500)).0, 2000, "medio segundo después, no");
    assert!(r.obtener(&format!("{otro}/a"), 1500).is_ok(), "el otro origen lleva su propia cuenta");
}

#[test]
fn crawl_delay_alarga_el_intervalo_pero_nunca_lo_acorta() {
    let largo = Falso::default().con(&format!("{SITIO}/robots.txt"), 200, "User-agent: *\nCrawl-delay: 5\n");
    let mut r = rastreador(&largo);
    assert_eq!(esperar_hasta(r.obtener(&format!("{SITIO}/a"), 0)), (5000, MotivoEspera::CrawlDelay));

    let corto = Falso::default().con(&format!("{SITIO}/robots.txt"), 200, "User-agent: *\nCrawl-delay: 0.1\n");
    let mut r = rastreador(&corto);
    assert_eq!(esperar_hasta(r.obtener(&format!("{SITIO}/a"), 0)), (1000, MotivoEspera::Intervalo));
}

#[test]
fn el_presupuesto_por_origen_se_agota() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "");
    let mut r = Rastreador::nuevo(&f, identidad(), Config { max_paginas_por_origen: 2, ..config() });
    let mut t = 0;
    let _ = r.obtener(&format!("{SITIO}/a"), t);
    for _ in 0..2 {
        t += 1000;
        assert!(r.obtener(&format!("{SITIO}/a"), t).is_ok());
    }
    t += 1000;
    assert_eq!(r.obtener(&format!("{SITIO}/a"), t), Err(Rechazo::PresupuestoAgotado { paginas: 2 }));
}

// ─── Respuestas que piden esperar ────────────────────────────────────────

#[test]
fn retry_after_se_respeta_tal_cual_aunque_supere_el_tope_del_backoff() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "").con_respuesta(
        &format!("{SITIO}/a"),
        Respuesta { estado: 429, retry_after: Some("120".into()), ..Default::default() },
    );
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/a"), 0);
    let p = r.obtener(&format!("{SITIO}/a"), 1000).expect("respuesta 429");
    assert_eq!(p.espera_hasta_ms, Some(121_000));
    assert_eq!(esperar_hasta(r.obtener(&format!("{SITIO}/b"), 60_000)), (121_000, MotivoEspera::Backoff));
}

#[test]
fn el_backoff_se_duplica_tiene_tope_y_se_reinicia_con_un_exito() {
    let f = Falso::default()
        .con(&format!("{SITIO}/robots.txt"), 404, "")
        .con(&format!("{SITIO}/falla"), 503, "")
        .con(&format!("{SITIO}/ok"), 200, "");
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/falla"), 0);

    let mut t = 1000;
    let mut esperas = Vec::new();
    for _ in 0..5 {
        let hasta = r.obtener(&format!("{SITIO}/falla"), t).expect("503").espera_hasta_ms.expect("espera");
        esperas.push(hasta - t);
        t = hasta;
    }
    assert_eq!(esperas, vec![2000, 4000, 8000, 16_000, 16_000], "se duplica y se queda en el tope");

    // Una respuesta buena reinicia la cuenta: la siguiente falla vuelve a la espera inicial.
    assert_eq!(r.obtener(&format!("{SITIO}/ok"), t).map(|p| p.estado), Ok(200));
    t += 1000;
    let hasta = r.obtener(&format!("{SITIO}/falla"), t).expect("503").espera_hasta_ms.expect("espera");
    assert_eq!(hasta - t, 2000);
}

#[test]
fn una_falla_de_red_es_un_rechazo_con_backoff() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "").con_falla(&format!("{SITIO}/a"));
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/a"), 0);
    match r.obtener(&format!("{SITIO}/a"), 1000) {
        Err(Rechazo::FallaDeRed { desde_ms, .. }) => assert_eq!(desde_ms, 3000),
        otro => panic!("{otro:?}"),
    }
}

// ─── Redirecciones e identidad ───────────────────────────────────────────

#[test]
fn una_redireccion_se_devuelve_y_no_se_sigue_sola() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "").con_respuesta(
        &format!("{SITIO}/viejo"),
        Respuesta { estado: 302, location: Some("https://otro.test/nuevo".into()), ..Default::default() },
    );
    let mut r = rastreador(&f);
    let _ = r.obtener(&format!("{SITIO}/viejo"), 0);
    let p = r.obtener(&format!("{SITIO}/viejo"), 1000).expect("302");
    assert_eq!(p.redireccion.as_ref().map(Url::as_str), Some("https://otro.test/nuevo"));
    assert!(
        f.pedidas.borrow().iter().all(|(u, _)| !u.contains("otro.test")),
        "el destino no se pidió: pasará por su propio robots.txt cuando se lo pida"
    );
}

#[test]
fn cada_peticion_lleva_la_misma_identidad_declarada() {
    let f = Falso::default().con(&format!("{SITIO}/robots.txt"), 404, "");
    let mut r = rastreador(&f);
    let mut t = 0;
    for ruta in ["/a", "/b", "/c"] {
        while let Err(Rechazo::Esperar { desde_ms, .. }) = r.obtener(&format!("{SITIO}{ruta}"), t) {
            t = desde_ms;
        }
    }
    let agentes: Vec<String> = f.pedidas.borrow().iter().map(|(_, a)| a.clone()).collect();
    assert_eq!(agentes.len(), 4, "robots.txt y tres páginas");
    assert!(agentes.iter().all(|a| a == "Lector/1.0 (+https://lector.test/bot)"), "{agentes:?}");
}

#[test]
fn solo_http_y_https() {
    let f = Falso::default();
    let mut r = rastreador(&f);
    assert!(matches!(r.obtener("ftp://sitio.test/a", 0), Err(Rechazo::EsquemaNoSoportado(_))));
    assert!(matches!(r.obtener("no es una url", 0), Err(Rechazo::UrlInvalida(_))));
    assert!(f.pedidas.borrow().is_empty());
}
