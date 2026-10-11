//! El servidor HTTP: dirección solo de loopback, límites, contrabando de peticiones y
//! defensas contra el navegador. Todo contra un servidor real en `127.0.0.1`, puerto 0.

mod comun;

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use comun::{crudo, esperar_hasta, get, leer_respuesta, post, Respuesta};
use ops_console::defensas::hosts_validos;
use ops_console::direccion::{DireccionLocal, ErrorDireccion};
use ops_console::http::{self, Limites, Peticion, CABECERAS_DE_SEGURIDAD};
use ops_console::json::Json;
use ops_console::servidor::{Manejador, Servidor};

/// Devuelve la ruta y el cuerpo que recibió: así se ve qué llegó a la aplicación.
struct Eco;

impl Manejador for Eco {
    fn atender(&self, peticion: &Peticion) -> http::Respuesta {
        http::Respuesta::ok(Json::objeto([
            ("ruta", peticion.cabeza.ruta.as_str().into()),
            (
                "cuerpo",
                String::from_utf8_lossy(&peticion.cuerpo)
                    .into_owned()
                    .into(),
            ),
        ]))
    }
}

fn limites_cortos() -> Limites {
    Limites {
        tiempo_lectura: Duration::from_millis(600),
        ..Limites::default()
    }
}

fn servidor_con(limites: Limites) -> Servidor {
    let direccion = DireccionLocal::desde_texto("127.0.0.1:0").expect("dirección");
    Servidor::escuchar(direccion, limites, Arc::new(Eco)).expect("escuchar")
}

fn servidor() -> Servidor {
    servidor_con(limites_cortos())
}

/// Una petición con la línea y el `Host` correctos más `resto` (cabeceras y cuerpo).
fn con_host(direccion: SocketAddr, linea: &str, resto: &str) -> Respuesta {
    crudo(
        direccion,
        format!("{linea}\r\nHost: {direccion}\r\n{resto}").as_bytes(),
    )
}

/// Las cabeceras de un `POST` correcto, sin `Content-Length`.
fn cabeceras_post(direccion: SocketAddr) -> String {
    format!("Content-Type: application/json\r\nX-Consola: 1\r\nOrigin: http://{direccion}\r\n")
}

fn assert_estado(r: &Respuesta, estado: u16, codigo: &str) {
    assert_eq!(r.estado, estado, "cuerpo: {}", r.texto());
    assert_eq!(r.codigo(), codigo);
}

// ─── Solo loopback ────────────────────────────────────────────────────────────────────────

#[test]
fn solo_una_direccion_de_loopback_es_una_direccion_local() {
    for texto in [
        "127.0.0.1:0",
        "127.8.9.10:8750",
        "[::1]:8750",
        "localhost:8750",
        "LocalHost:1",
    ] {
        let direccion = DireccionLocal::desde_texto(texto)
            .unwrap_or_else(|e| panic!("{texto} debería valer: {e}"));
        assert!(direccion.socket().ip().is_loopback());
    }
    assert_eq!(
        DireccionLocal::desde_texto("localhost:8750").map(|d| d.socket()),
        Ok("127.0.0.1:8750".parse().expect("socket")),
        "localhost se toma como 127.0.0.1 sin consultar ningún resolvedor"
    );
}

#[test]
fn cero_una_ip_publica_y_un_localhost_mal_escrito_se_rechazan_antes_de_abrir_nada() {
    // Ninguna de estas llega a ser una `DireccionLocal`, y `Servidor::escuchar` no acepta
    // otra cosa: el puerto ni siquiera se intenta abrir.
    for (texto, ip) in [
        ("0.0.0.0:8750", "0.0.0.0"),
        ("[::]:8750", "::"),
        ("192.0.2.10:8750", "192.0.2.10"),
        ("203.0.113.5:80", "203.0.113.5"),
        ("[2001:db8::1]:8750", "2001:db8::1"),
        ("[::ffff:127.0.0.1]:8750", "::ffff:127.0.0.1"),
    ] {
        let error = DireccionLocal::desde_texto(texto).expect_err(texto);
        assert_eq!(
            error,
            ErrorDireccion::NoLocal(ip.parse().expect("ip")),
            "{texto}"
        );
        assert!(error.to_string().contains("loopback"), "{error}");
    }
    for texto in [
        "localhots:8750",
        "localhost.:8750",
        "local host:8750",
        "localhost",
        "localhost:99999",
        "127.0.0.1",
        "::1:8750",
        "ejemplo.example:8750",
    ] {
        assert!(
            matches!(
                DireccionLocal::desde_texto(texto),
                Err(ErrorDireccion::Ilegible(_))
            ),
            "{texto}"
        );
    }
    assert_eq!(
        DireccionLocal::nueva("0.0.0.0:0".parse().expect("socket")),
        Err(ErrorDireccion::NoLocal("0.0.0.0".parse().expect("ip")))
    );
}

#[test]
fn el_binario_se_niega_a_escuchar_fuera_de_loopback_y_dice_por_que() {
    for (direccion, motivo) in [
        ("0.0.0.0:8750", "no es de esta máquina"),
        ("192.0.2.10:8750", "no es de esta máquina"),
        ("localhots:8750", "no es una dirección"),
    ] {
        let salida = Command::new(env!("CARGO_BIN_EXE_ops-console"))
            .args([
                "--escuchar",
                direccion,
                "--trabajador",
                "x",
                "--registros",
                "y",
            ])
            .output()
            .expect("ejecutar ops-console");
        let error = String::from_utf8_lossy(&salida.stderr);
        assert_eq!(salida.status.code(), Some(2), "{direccion}: {error}");
        assert!(error.contains(motivo), "{direccion}: {error}");
        assert!(salida.stdout.is_empty(), "no llegó a atender nada");
    }
}

// ─── Una petición por conexión ────────────────────────────────────────────────────────────

#[test]
fn una_peticion_correcta_recibe_su_respuesta_y_la_conexion_se_cierra() {
    let s = servidor();
    let r = get(s.direccion(), "/hola?x=1");
    assert_eq!(r.estado, 200);
    assert_eq!(r.cabecera("connection"), Some("close"));
    assert_eq!(
        r.cabecera("content-length"),
        Some(r.cuerpo.len().to_string().as_str())
    );
    assert_eq!(
        r.json().campo("datos").and_then(|d| d.campo("ruta")),
        Some(&Json::from("/hola"))
    );
}

#[test]
fn una_segunda_peticion_en_la_misma_conexion_no_se_atiende() {
    let s = servidor();
    let d = s.direccion();
    let dos =
        format!("GET /uno HTTP/1.1\r\nHost: {d}\r\n\r\nGET /dos HTTP/1.1\r\nHost: {d}\r\n\r\n");
    let r = crudo(d, dos.as_bytes());
    assert_eq!(r.estado, 200);
    assert!(r.texto().contains("/uno"));
    assert!(
        !r.texto().contains("/dos") && !String::from_utf8_lossy(&r.cuerpo).contains("HTTP/1.1"),
        "sin keep-alive: solo la primera"
    );
}

#[test]
fn los_errores_tambien_cierran_y_llevan_las_cabeceras_de_seguridad() {
    let s = servidor();
    let d = s.direccion();
    let respuestas = [
        crudo(d, b"BASURA\r\n\r\n"),
        crudo(d, b"PUT / HTTP/1.1\r\nHost: x\r\n\r\n"),
        crudo(
            d,
            format!(
                "GET / HTTP/1.1\r\nHost: atacante.example:{}\r\n\r\n",
                d.port()
            )
            .as_bytes(),
        ),
        con_host(d, "POST / HTTP/1.1", "Content-Length: 2\r\n\r\n{}"),
        con_host(d, "GET / HTTP/1.1", "Transfer-Encoding: chunked\r\n\r\n"),
    ];
    for r in respuestas {
        assert!(r.estado >= 400, "{}", r.estado);
        assert_eq!(r.cabecera("connection"), Some("close"));
        for (nombre, valor) in CABECERAS_DE_SEGURIDAD {
            assert_eq!(
                r.cabecera(&nombre.to_ascii_lowercase()),
                Some(valor),
                "{nombre} en un {}",
                r.estado
            );
        }
        assert!(
            r.cabeceras
                .iter()
                .all(|(n, _)| !n.starts_with("access-control-")),
            "sin CORS"
        );
    }
}

// ─── Límites ──────────────────────────────────────────────────────────────────────────────

#[test]
fn una_linea_de_peticion_demasiado_larga_es_414() {
    let s = servidor();
    let ruta = format!("/{}", "a".repeat(Limites::default().linea_max));
    let r = con_host(s.direccion(), &format!("GET {ruta} HTTP/1.1"), "\r\n");
    assert_estado(&r, 414, "linea_demasiado_larga");
}

#[test]
fn una_linea_larga_se_rechaza_sin_esperar_al_final_de_la_cabeza() {
    // La línea nunca termina: el servidor no puede esperar a `\r\n\r\n`.
    let s = servidor();
    let mut flujo = TcpStream::connect(s.direccion()).expect("conectar");
    let ruta = "b".repeat(Limites::default().linea_max + 10);
    flujo
        .write_all(format!("GET /{ruta}").as_bytes())
        .expect("escribir");
    let inicio = Instant::now();
    let r = leer_respuesta(&mut flujo);
    assert_eq!(r.estado, 414);
    assert!(
        inicio.elapsed() < limites_cortos().tiempo_lectura,
        "no esperó al plazo"
    );
}

#[test]
fn cabeceras_que_superan_el_tope_de_bytes_son_431() {
    let s = servidor();
    let relleno: String = (0..20)
        .map(|i| format!("X-Relleno-{i}: {}\r\n", "r".repeat(500)))
        .collect();
    let r = con_host(s.direccion(), "GET / HTTP/1.1", &format!("{relleno}\r\n"));
    assert_estado(&r, 431, "cabeceras_demasiado_grandes");
}

#[test]
fn demasiadas_cabeceras_son_431() {
    let s = servidor();
    let muchas: String = (0..Limites::default().cabeceras_max + 1)
        .map(|i| format!("X-C{i}: 1\r\n"))
        .collect();
    let r = con_host(s.direccion(), "GET / HTTP/1.1", &format!("{muchas}\r\n"));
    assert_estado(&r, 431, "demasiadas_cabeceras");
}

#[test]
fn un_cuerpo_anunciado_mayor_que_el_tope_es_413_sin_leer_un_byte() {
    // El cuerpo no se manda nunca: si el servidor intentara leerlo, respondería 408 al
    // vencer el plazo. Responde 413 antes, con solo mirar `Content-Length`.
    let s = servidor();
    let d = s.direccion();
    let tope = Limites::default().cuerpo_max;
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        &format!("{}Content-Length: {}\r\n\r\n", cabeceras_post(d), tope + 1),
    );
    assert_estado(&r, 413, "cuerpo_demasiado_grande");

    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        &format!(
            "{}Content-Length: 999999999999999999\r\n\r\n",
            cabeceras_post(d)
        ),
    );
    assert_estado(&r, 413, "cuerpo_demasiado_grande");
}

#[test]
fn con_un_content_length_mentiroso_no_se_lee_de_mas() {
    // Anuncia 2 bytes y manda 40 KB: la aplicación recibe exactamente los 2 bytes y la
    // basura que sigue no se interpreta como nada.
    let s = servidor();
    let d = s.direccion();
    let basura = "X".repeat(40 * 1024);
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        &format!("{}Content-Length: 2\r\n\r\n{{}}{basura}", cabeceras_post(d)),
    );
    assert_eq!(r.estado, 200, "{}", r.texto());
    assert_eq!(
        r.json().campo("datos").and_then(|d| d.campo("cuerpo")),
        Some(&Json::from("{}"))
    );
}

#[test]
fn un_cuerpo_que_promete_mas_de_lo_que_llega_vence_el_plazo_408() {
    let s = servidor();
    let d = s.direccion();
    let mut flujo = TcpStream::connect(d).expect("conectar");
    flujo
        .write_all(
            format!(
                "POST /x HTTP/1.1\r\nHost: {d}\r\n{}Content-Length: 10\r\n\r\n{{\"a\"",
                cabeceras_post(d)
            )
            .as_bytes(),
        )
        .expect("escribir");
    let r = leer_respuesta(&mut flujo);
    assert_estado(&r, 408, "tiempo_agotado");
}

#[test]
fn un_post_sin_content_length_es_411() {
    let s = servidor();
    let d = s.direccion();
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        &format!("{}\r\n{{}}", cabeceras_post(d)),
    );
    assert_estado(&r, 411, "falta_content_length");
}

#[test]
fn un_cliente_mudo_recibe_408_al_vencer_el_plazo() {
    let s = servidor();
    let mut flujo = TcpStream::connect(s.direccion()).expect("conectar");
    let r = leer_respuesta(&mut flujo);
    assert_estado(&r, 408, "tiempo_agotado");
}

#[test]
fn un_cliente_que_gotea_bytes_no_estira_el_plazo_ni_retiene_su_lugar() {
    // Un byte cada 50 ms, durante segundos: cada lectura recibe algo, pero el plazo es total.
    // Y después del 408 el cliente sigue goteando: aun así, su lugar se libera enseguida.
    let s = servidor();
    let d = s.direccion();
    let mut flujo = TcpStream::connect(d).expect("conectar");
    let mut lector = flujo.try_clone().expect("clonar");
    let basta = Arc::new(AtomicBool::new(false));
    let goteo = {
        let basta = Arc::clone(&basta);
        thread::spawn(move || {
            let _ = flujo.write_all(format!("GET / HTTP/1.1\r\nHost: {d}\r\nX-Goteo: ").as_bytes());
            while !basta.load(Ordering::SeqCst) {
                if flujo.write_all(b"g").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        })
    };
    let inicio = Instant::now();
    let r = leer_respuesta(&mut lector);
    assert_estado(&r, 408, "tiempo_agotado");
    assert!(
        inicio.elapsed() < Duration::from_secs(5),
        "respondió a los {:?}, con el cliente todavía goteando",
        inicio.elapsed()
    );
    assert!(
        esperar_hasta(Duration::from_secs(2), || s.conexiones_activas() == 0),
        "el cliente sigue goteando y su lugar no se liberó"
    );
    basta.store(true, Ordering::SeqCst);
    goteo.join().expect("goteo");
}

#[test]
fn pasado_el_tope_de_conexiones_la_siguiente_recibe_503() {
    let s = servidor_con(Limites {
        conexiones_max: 2,
        tiempo_lectura: Duration::from_secs(3),
        ..Limites::default()
    });
    let d = s.direccion();
    let ocupadas: Vec<TcpStream> = (0..2)
        .map(|_| TcpStream::connect(d).expect("conectar"))
        .collect();
    assert!(
        esperar_hasta(Duration::from_secs(5), || s.conexiones_activas() == 2),
        "las dos conexiones mudas ocupan sus lugares"
    );
    let r = get(d, "/");
    assert_estado(&r, 503, "servidor_ocupado");
    assert_eq!(r.cabecera("retry-after"), Some("2"));

    drop(ocupadas);
    assert!(
        esperar_hasta(Duration::from_secs(5), || s.conexiones_activas() == 0),
        "al cerrarse, liberan su lugar"
    );
    assert_eq!(get(d, "/").estado, 200);
}

// ─── Protocolo estricto y contrabando ─────────────────────────────────────────────────────

#[test]
fn content_length_repetido_o_en_conflicto_es_400() {
    let s = servidor();
    let d = s.direccion();
    for (cabeceras, codigo) in [
        (
            "Content-Length: 2\r\nContent-Length: 2\r\n",
            "content_length_repetido",
        ),
        (
            "Content-Length: 2\r\nContent-Length: 3\r\n",
            "content_length_repetido",
        ),
        (
            "Content-Length: 2\r\ncontent-length: 2\r\n",
            "content_length_repetido",
        ),
        ("Content-Length: 2, 2\r\n", "content_length_invalido"),
        ("Content-Length: +2\r\n", "content_length_invalido"),
        ("Content-Length: -2\r\n", "content_length_invalido"),
        ("Content-Length: 0x2\r\n", "content_length_invalido"),
        (
            "Content-Length: 1234567890123456789\r\n",
            "content_length_invalido",
        ),
    ] {
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!("{}{cabeceras}\r\n{{}}", cabeceras_post(d)),
        );
        assert_eq!(r.estado, 400, "{cabeceras:?}: {}", r.texto());
        assert_eq!(r.codigo(), codigo, "{cabeceras:?}");
    }
}

#[test]
fn transfer_encoding_en_cualquier_forma_es_501() {
    let s = servidor();
    let d = s.direccion();
    for cabeceras in [
        "Transfer-Encoding: chunked\r\n",
        "TRANSFER-ENCODING: chunked\r\n",
        "Transfer-Encoding: identity\r\n",
        "Transfer-Encoding: chunked\r\nContent-Length: 2\r\n",
        "Content-Length: 2\r\nTransfer-Encoding: gzip, chunked\r\n",
    ] {
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!("{}{cabeceras}\r\n{{}}", cabeceras_post(d)),
        );
        assert_eq!(r.estado, 501, "{cabeceras:?}");
        assert_eq!(r.codigo(), "transfer_encoding_no_soportado");
    }
}

#[test]
fn lo_que_no_se_entiende_entero_se_rechaza() {
    let s = servidor();
    let d = s.direccion();
    let h = format!("Host: {d}");
    for (peticion, estado, codigo) in [
        (format!("GET / HTTP/1.1\r\nX-Plegada: a\r\n b\r\n{h}\r\n\r\n"), 400, "cabecera_plegada"),
        (format!("GET / HTTP/1.1\r\nX-Espacio : a\r\n{h}\r\n\r\n"), 400, "cabecera_malformada"),
        (format!("GET / HTTP/1.1\r\nSin-dos-puntos\r\n{h}\r\n\r\n"), 400, "cabecera_malformada"),
        (format!("GET / HTTP/1.1\r\nX-Control: a\u{1}b\r\n{h}\r\n\r\n"), 400, "cabecera_malformada"),
        (format!("GET / HTTP/1.1\n{h}\n\n"), 400, "fin_de_linea_invalido"),
        (format!("GET  / HTTP/1.1\r\n{h}\r\n\r\n"), 400, "linea_malformada"),
        (format!("GET http://{d}/ HTTP/1.1\r\n{h}\r\n\r\n"), 400, "ruta_invalida"),
        (format!("GET / HTTP/2.0\r\n{h}\r\n\r\n"), 505, "version_no_soportada"),
        (format!("GET / HTTQ/1.1\r\n{h}\r\n\r\n"), 400, "version_invalida"),
        (format!("G(T / HTTP/1.1\r\n{h}\r\n\r\n"), 400, "metodo_invalido"),
        ("GET / HTTP/1.1\r\n\r\n".to_string(), 400, "sin_host"),
        (format!("GET / HTTP/1.1\r\n{h}\r\n{h}\r\n\r\n"), 400, "host_repetido"),
        (format!("GET / HTTP/1.1\r\n{h}\r\nContent-Length: 1\r\n\r\nx"), 400, "get_con_cuerpo"),
        (format!("POST /x HTTP/1.1\r\n{h}\r\n{}Expect: 100-continue\r\nContent-Length: 2\r\n\r\n{{}}", cabeceras_post(d)), 417, "expect_no_soportado"),
    ] {
        let r = crudo(d, peticion.as_bytes());
        assert_eq!(r.estado, estado, "{peticion:?}: {}", r.texto());
        assert_eq!(r.codigo(), codigo, "{peticion:?}");
    }
}

#[test]
fn solo_get_y_post_y_sin_preflight_cors() {
    let s = servidor();
    let d = s.direccion();
    for metodo in [
        "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD", "TRACE", "CONNECT",
    ] {
        let r = con_host(
            d,
            &format!("{metodo} / HTTP/1.1"),
            "Origin: http://atacante.example\r\nAccess-Control-Request-Method: POST\r\n\r\n",
        );
        assert_eq!(r.estado, 405, "{metodo}");
        assert_eq!(r.cabecera("allow"), Some("GET, POST"));
        assert!(
            r.cabeceras
                .iter()
                .all(|(n, _)| !n.starts_with("access-control-")),
            "{metodo}: ninguna cabecera CORS"
        );
    }
}

// ─── Defensas contra el navegador ─────────────────────────────────────────────────────────

#[test]
fn un_host_ajeno_es_421_aunque_la_conexion_llegue_a_loopback() {
    // Rebinding de DNS: la página de atacante.example habla con 127.0.0.1, pero su
    // navegador manda `Host: atacante.example`.
    let s = servidor();
    let d = s.direccion();
    for host in [
        "atacante.example".to_string(),
        format!("atacante.example:{}", d.port()),
        format!("127.0.0.1:{}", d.port() ^ 1),
        format!("localhost:{}", d.port() ^ 1),
        "127.0.0.1".to_string(),
        "localhost".to_string(),
        format!("127.0.0.2:{}", d.port()),
        format!("0.0.0.0:{}", d.port()),
    ] {
        let r = crudo(
            d,
            format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes(),
        );
        assert_eq!(r.estado, 421, "Host: {host}");
        assert_eq!(r.codigo(), "host_ajeno");

        let r = crudo(
            d,
            format!(
                "POST /x HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
                 X-Consola: 1\r\nContent-Length: 2\r\n\r\n{{}}"
            )
            .as_bytes(),
        );
        assert_eq!(r.estado, 421, "POST con Host: {host}");
    }
}

#[test]
fn los_nombres_propios_del_servidor_se_aceptan() {
    let s = servidor();
    let d = s.direccion();
    for host in [
        d.to_string(),
        format!("localhost:{}", d.port()),
        format!("LOCALHOST:{}", d.port()),
        format!("[::1]:{}", d.port()),
    ] {
        let r = crudo(
            d,
            format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes(),
        );
        assert_eq!(r.estado, 200, "Host: {host}");
    }
}

#[test]
fn en_el_puerto_80_el_host_tambien_vale_sin_puerto() {
    let en_80 = hosts_validos("127.0.0.1:80".parse().expect("socket"));
    for host in [
        "127.0.0.1:80",
        "localhost:80",
        "[::1]:80",
        "127.0.0.1",
        "localhost",
        "[::1]",
    ] {
        assert!(en_80.iter().any(|h| h == host), "{host}");
    }
    let en_8750 = hosts_validos("127.0.0.1:8750".parse().expect("socket"));
    assert!(!en_8750.iter().any(|h| h == "localhost" || h == "127.0.0.1"));
    let propia = hosts_validos("127.0.0.7:8750".parse().expect("socket"));
    assert!(propia.iter().any(|h| h == "127.0.0.7:8750"));
}

#[test]
fn un_post_que_no_es_json_es_415() {
    let s = servidor();
    let d = s.direccion();
    for tipo in [
        None,
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        Some("multipart/form-data; boundary=x"),
        Some("application/json-patch+json"),
        Some("text/json"),
    ] {
        let tipo = tipo
            .map(|t| format!("Content-Type: {t}\r\n"))
            .unwrap_or_default();
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!("{tipo}X-Consola: 1\r\nContent-Length: 2\r\n\r\n{{}}"),
        );
        assert_estado(&r, 415, "tipo_no_admitido");
    }
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        "Content-Type: Application/JSON; charset=utf-8\r\nX-Consola: 1\r\nContent-Length: 2\r\n\r\n{}",
    );
    assert_eq!(r.estado, 200, "JSON con parámetros pasa: {}", r.texto());
}

#[test]
fn un_post_sin_la_cabecera_propia_es_403() {
    let s = servidor();
    let d = s.direccion();
    for propia in ["", "X-Consola: 0\r\n", "X-Consola: si\r\n", "X-Otra: 1\r\n"] {
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!("Content-Type: application/json\r\n{propia}Content-Length: 2\r\n\r\n{{}}"),
        );
        assert_estado(&r, 403, "falta_cabecera_propia");
    }
}

#[test]
fn un_post_de_otro_origen_es_403() {
    let s = servidor();
    let d = s.direccion();
    for origen in [
        "Origin: http://atacante.example\r\n".to_string(),
        "Origin: null\r\n".to_string(),
        format!("Origin: https://{d}\r\n"),
        format!("Origin: http://127.0.0.1:{}\r\n", d.port() ^ 1),
        format!("Origin: http://{d}.atacante.example\r\n"),
        "Sec-Fetch-Site: cross-site\r\n".to_string(),
        "Sec-Fetch-Site: same-site\r\n".to_string(),
    ] {
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!(
                "Content-Type: application/json\r\nX-Consola: 1\r\n{origen}Content-Length: 2\r\n\r\n{{}}"
            ),
        );
        assert_estado(&r, 403, "origen_ajeno");
    }
}

#[test]
fn un_post_del_mismo_origen_pasa() {
    let s = servidor();
    let d = s.direccion();
    for extra in [
        String::new(),
        format!("Origin: http://{d}\r\n"),
        format!("Origin: http://{d}\r\nSec-Fetch-Site: same-origin\r\n"),
        "Sec-Fetch-Site: none\r\n".to_string(),
    ] {
        let r = con_host(
            d,
            "POST /x HTTP/1.1",
            &format!(
                "Content-Type: application/json\r\nX-Consola: 1\r\n{extra}Content-Length: 2\r\n\r\n{{}}"
            ),
        );
        assert_eq!(r.estado, 200, "{extra:?}: {}", r.texto());
    }
    assert_eq!(post(d, "/x", "{}").estado, 200);
}

#[test]
fn las_defensas_se_aplican_antes_de_leer_el_cuerpo() {
    // El cuerpo anunciado no llega nunca: si el servidor lo esperara, respondería 408.
    let s = servidor();
    let d = s.direccion();
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        "Content-Type: text/plain\r\nContent-Length: 100\r\n\r\n",
    );
    assert_eq!(r.estado, 415);
    let r = con_host(
        d,
        "POST /x HTTP/1.1",
        "Content-Type: application/json\r\nContent-Length: 100\r\n\r\n",
    );
    assert_eq!(r.estado, 403);
}

#[test]
fn soltar_el_servidor_deja_de_atender() {
    let s = servidor();
    let d = s.direccion();
    assert_eq!(get(d, "/").estado, 200);
    drop(s);
    let atendida = TcpStream::connect_timeout(&d, Duration::from_millis(500)).map(|mut f| {
        let _ = f.write_all(format!("GET / HTTP/1.1\r\nHost: {d}\r\n\r\n").as_bytes());
        let _ = f.set_read_timeout(Some(Duration::from_millis(500)));
        let mut uno = [0u8; 1];
        matches!(std::io::Read::read(&mut f, &mut uno), Ok(1))
    });
    assert!(
        !atendida.unwrap_or(false),
        "nadie responde después de soltarlo"
    );
}
