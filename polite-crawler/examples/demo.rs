//! Recorre un sitio simulado en este mismo proceso, con un reloj real, y cuenta
//! cada decisión del rastreador. No necesita internet.
//!
//! ```text
//! cargo run --example demo
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use polite_crawler::{Config, Identidad, Rastreador, Rechazo, TransporteHttp};

const ROBOTS: &str = "User-agent: *\nDisallow: /borradores/\nAllow: /borradores/publicados/\nCrawl-delay: 0.6\n";

fn sitio_simulado() -> std::io::Result<String> {
    let oyente = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", oyente.local_addr()?);
    thread::spawn(move || {
        let mut ocupado_una_vez = true;
        for mut flujo in oyente.incoming().flatten() {
            let mut linea = String::new();
            let mut lector = BufReader::new(match flujo.try_clone() {
                Ok(f) => f,
                Err(_) => continue,
            });
            if lector.read_line(&mut linea).is_err() {
                continue;
            }
            // Descarta los encabezados de la petición.
            let mut h = String::new();
            while lector.read_line(&mut h).is_ok_and(|n| n > 2) {
                h.clear();
            }
            let ruta = linea.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (estado, extra, cuerpo) = match ruta.as_str() {
                "/robots.txt" => ("200 OK", String::new(), ROBOTS.to_string()),
                "/noticias" if ocupado_una_vez => {
                    ocupado_una_vez = false;
                    ("429 Too Many Requests", "Retry-After: 2\r\n".to_string(), String::new())
                }
                "/viejo" => ("301 Moved Permanently", "Location: /noticias\r\n".to_string(), String::new()),
                "/borradores/publicados/manual" | "/noticias" | "/" => ("200 OK", String::new(), format!("página {ruta}")),
                _ => ("404 Not Found", String::new(), String::new()),
            };
            let _ = write!(
                flujo,
                "HTTP/1.1 {estado}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{cuerpo}",
                cuerpo.len()
            );
        }
    });
    Ok(base)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = sitio_simulado()?;
    let identidad = Identidad::nueva("LectorDemo", "0.1", "https://ejemplo.test/bot")?;
    println!("Me presento como: {}\n", identidad.user_agent());

    let config = Config { intervalo_minimo: Duration::from_millis(300), ..Config::default() };
    let mut rastreador = Rastreador::nuevo(TransporteHttp::nuevo(Duration::from_secs(5), 1 << 20), identidad, config);

    let inicio = Instant::now();
    let reloj = || u64::try_from(inicio.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut pendientes: Vec<String> =
        ["/", "/borradores/idea", "/borradores/publicados/manual", "/viejo", "/noticias"].iter().map(|r| format!("{base}{r}")).collect();
    pendientes.reverse();

    while let Some(url) = pendientes.pop() {
        let ruta = url.replace(&base, "");
        match rastreador.obtener(&url, reloj()) {
            Ok(p) => {
                let t = reloj();
                match (p.estado, &p.redireccion, p.espera_hasta_ms) {
                    (_, Some(destino), _) => {
                        println!("{t:>6} ms  {ruta:<32} {} → redirige a {} (se pide aparte)", p.estado, destino.path());
                        pendientes.push(destino.to_string());
                    }
                    (_, _, Some(hasta)) => {
                        println!("{t:>6} ms  {ruta:<32} {} → el servidor pide esperar hasta {hasta} ms", p.estado);
                        pendientes.push(url);
                    }
                    _ => println!("{t:>6} ms  {ruta:<32} {} · {:?}", p.estado, String::from_utf8_lossy(&p.cuerpo)),
                }
            }
            Err(Rechazo::Esperar { desde_ms, motivo }) => {
                thread::sleep(Duration::from_millis(desde_ms.saturating_sub(reloj())));
                println!("{:>6} ms  (esperé por {motivo:?})", reloj());
                pendientes.push(url);
            }
            Err(otro) => println!("{:>6} ms  {ruta:<32} no: {otro}", reloj()),
        }
    }
    Ok(())
}
