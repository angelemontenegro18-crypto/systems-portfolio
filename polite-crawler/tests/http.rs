//! El transporte HTTP real contra un servidor en el mismo proceso: lo que viaja
//! por el cable es lo que el código dice.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use polite_crawler::{Config, Identidad, Rastreador, Rechazo, Transporte, TransporteHttp};
use url::Url;

/// Petición recibida: ruta y User-Agent.
type Registro = Arc<Mutex<Vec<(String, String)>>>;

/// Un sitio mínimo: responde según la ruta y anota cada petición.
fn sitio() -> (String, Registro) {
    let oyente = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", oyente.local_addr().expect("dirección"));
    let registro: Registro = Arc::default();
    let reg = Arc::clone(&registro);
    thread::spawn(move || {
        for mut flujo in oyente.incoming().flatten() {
            let mut lector = BufReader::new(flujo.try_clone().expect("clonar"));
            let mut linea = String::new();
            if lector.read_line(&mut linea).is_err() {
                continue;
            }
            let ruta = linea.split_whitespace().nth(1).unwrap_or("/").to_string();
            let mut agente = String::new();
            loop {
                let mut h = String::new();
                if lector.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.eq_ignore_ascii_case("user-agent") {
                        agente = v.trim().to_string();
                    }
                }
            }
            reg.lock().expect("registro").push((ruta.clone(), agente));

            let (estado, extra, cuerpo): (&str, &str, String) = match ruta.as_str() {
                "/robots.txt" => (
                    "200 OK",
                    "",
                    "User-agent: lector\nDisallow: /privado\n".into(),
                ),
                "/pagina" => ("200 OK", "", "contenido".into()),
                "/grande" => ("200 OK", "", "x".repeat(10_000)),
                "/mudada" => ("302 Found", "Location: /pagina\r\n", String::new()),
                "/ocupado" => (
                    "429 Too Many Requests",
                    "Retry-After: 30\r\n",
                    String::new(),
                ),
                _ => ("404 Not Found", "", String::new()),
            };
            let respuesta = format!(
                "HTTP/1.1 {estado}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{cuerpo}",
                cuerpo.len()
            );
            let _ = flujo.write_all(respuesta.as_bytes());
        }
    });
    (base, registro)
}

fn identidad() -> Identidad {
    Identidad::nueva("Lector", "2.1", "equipo@lector.test").expect("identidad")
}

#[test]
fn el_user_agent_que_llega_al_servidor_es_la_identidad_declarada() {
    let (base, registro) = sitio();
    let t = TransporteHttp::nuevo(Duration::from_secs(5), 1 << 20);
    let r = t
        .get(
            &Url::parse(&format!("{base}/pagina")).expect("url"),
            identidad().user_agent(),
        )
        .expect("get");
    assert_eq!((r.estado, r.cuerpo.as_slice()), (200, &b"contenido"[..]));
    assert_eq!(
        registro.lock().expect("registro")[0].1,
        "Lector/2.1 (+equipo@lector.test)"
    );
}

#[test]
fn el_transporte_no_sigue_redirecciones() {
    let (base, registro) = sitio();
    let t = TransporteHttp::nuevo(Duration::from_secs(5), 1 << 20);
    let r = t
        .get(
            &Url::parse(&format!("{base}/mudada")).expect("url"),
            "Lector/1 (+x@y.test)",
        )
        .expect("get");
    assert_eq!(r.estado, 302);
    assert_eq!(r.location.as_deref(), Some("/pagina"));
    let rutas: Vec<String> = registro
        .lock()
        .expect("registro")
        .iter()
        .map(|(r, _)| r.clone())
        .collect();
    assert_eq!(rutas, vec!["/mudada"], "el destino no se pidió");
}

#[test]
fn los_4xx_y_el_retry_after_llegan_como_respuesta() {
    let (base, _) = sitio();
    let t = TransporteHttp::nuevo(Duration::from_secs(5), 1 << 20);
    let r = t
        .get(
            &Url::parse(&format!("{base}/ocupado")).expect("url"),
            "Lector/1 (+x@y.test)",
        )
        .expect("get");
    assert_eq!((r.estado, r.retry_after.as_deref()), (429, Some("30")));
    let r = t
        .get(
            &Url::parse(&format!("{base}/no-existe")).expect("url"),
            "Lector/1 (+x@y.test)",
        )
        .expect("get");
    assert_eq!(r.estado, 404);
}

#[test]
fn el_cuerpo_se_recorta_al_maximo() {
    let (base, _) = sitio();
    let t = TransporteHttp::nuevo(Duration::from_secs(5), 100);
    let r = t
        .get(
            &Url::parse(&format!("{base}/grande")).expect("url"),
            "Lector/1 (+x@y.test)",
        )
        .expect("get");
    assert_eq!(r.cuerpo.len(), 100);
    assert!(r.recortado);
}

#[test]
fn de_punta_a_punta_robots_se_respeta_contra_un_servidor_real() {
    let (base, registro) = sitio();
    let transporte = TransporteHttp::nuevo(Duration::from_secs(5), 1 << 20);
    let config = Config {
        intervalo_minimo: Duration::from_millis(10),
        ..Config::default()
    };
    let mut r = Rastreador::nuevo(transporte, identidad(), config);

    let mut t = 0;
    let pagina = loop {
        match r.obtener(&format!("{base}/pagina"), t) {
            Err(Rechazo::Esperar { desde_ms, .. }) => t = desde_ms,
            otro => break otro.expect("página"),
        }
    };
    assert_eq!(pagina.cuerpo, b"contenido");
    assert!(matches!(
        r.obtener(&format!("{base}/privado/x"), t + 1000),
        Err(Rechazo::ProhibidoPorRobots { .. })
    ));

    let rutas: Vec<String> = registro
        .lock()
        .expect("registro")
        .iter()
        .map(|(r, _)| r.clone())
        .collect();
    assert_eq!(
        rutas,
        vec!["/robots.txt", "/pagina"],
        "/privado nunca llegó al servidor"
    );
}
