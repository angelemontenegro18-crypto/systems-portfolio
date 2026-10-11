//! La consola con valores de demostración: trabajadores de verdad, servicios inventados y un
//! registro inventado, lleno de secretos falsos, que la consola muestra tapados.
//!
//! ```text
//! cargo run --example demo                     # http://127.0.0.1:8750/
//! cargo run --example demo -- 127.0.0.1:9000   # en otra dirección (solo loopback)
//! ```
//!
//! - Los trabajadores son este mismo ejecutable en modo trabajador. Cada uno «se cae» solo al
//!   cabo de un rato, para que se vea al tick reemplazarlo.
//! - Los servicios (`miniaturas-*`) responden desde un vigía fijo: no se consulta systemd.
//! - El registro se escribe junto a este ejecutable y crece con una línea cada dos segundos.
//!   Todos sus «secretos» son inventados.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ops_console::api::{Consola, Piezas};
use ops_console::direccion::DireccionLocal;
use ops_console::grupo::{FabricaDeProcesos, Grupo, Politica};
use ops_console::http::Limites;
use ops_console::reloj::{Reloj, RelojReal};
use ops_console::servidor::Servidor;
use ops_console::sistema::EntornoFijo;
use ops_console::trabajador;
use ops_console::vigia::{EstadoServicio, VigiaFijo};

const MODO_TRABAJADOR: &str = "--trabajador-demo";

/// El registro de partida. Los valores sensibles son de mentira; los dominios de correo son
/// los reservados para ejemplos.
const REGISTRO_INICIAL: [&str; 18] = [
    "INFO  miniaturas::inicio  servicio de miniaturas arrancando (versión de demostración)",
    "INFO  miniaturas::config  MINIATURAS_CLAVE_API=sk-demo-0000000000falsa cargada",
    "DEBUG miniaturas::firma  leyendo /etc/secretos/firma-miniaturas.pem",
    "DEBUG miniaturas::firma  en la máquina de pruebas: C:\\Secretos\\firma-miniaturas.pem",
    "INFO  miniaturas::cola  tarea 18210 lista: 4000x3000 -> 400x300 en 42 ms",
    "INFO  miniaturas::api  petición de ana.lopez@example.com: GET /v1/miniaturas/8812?lado=256",
    "INFO  miniaturas::cola  tarea 18211 lista: 1080x1920 -> 144x256 en 17 ms",
    "WARN  miniaturas::origen  reintento contra https://lector:clave-de-mentira@origen.example/imagenes/8812.jpg",
    "DEBUG miniaturas::cdn  subiendo con Authorization: Bearer demo.eyJmYWxzbyI6dHJ1ZX0.firma-falsa",
    "INFO  miniaturas::cache  guardada portada@2x.png (31 KB)",
    "ERROR miniaturas::cuentas  cuota agotada para CLI-482913 (plan básico)",
    "INFO  miniaturas::api  GET /v1/cuentas/77120034/miniaturas -> 200 en 8 ms",
    "DEBUG miniaturas::cdn  respuesta: {\"token\": \"tok_demo_123456\", \"expira\": 3600}",
    "WARN  miniaturas::api  password=hunter2-de-mentira rechazada para soporte@example.org",
    "DEBUG miniaturas::cdn  renovando token=",
    "      demo-token-en-la-linea-siguiente",
    "INFO  miniaturas::cola  tarea 18212 lista: 800x600 -> 256x192 en 9 ms",
    "INFO  miniaturas::cola  sin tareas pendientes",
];

/// Lo que se agrega cada dos segundos; `{n}` es un número de tarea.
const REGISTRO_EN_VIVO: [&str; 8] = [
    "INFO  miniaturas::cola  tarea {n} lista: 3024x4032 -> 192x256 en 31 ms",
    "INFO  miniaturas::cola  tarea {n} lista: 1920x1080 -> 256x144 en 12 ms",
    "DEBUG miniaturas::cdn  subida de la tarea {n} con api_key=demo-clave-{n}",
    "INFO  miniaturas::api  petición de usuario{n}@example.net: GET /v1/miniaturas/{n}",
    "WARN  miniaturas::origen  la tarea {n} tardó 1250 ms en leer el original",
    "INFO  miniaturas::cache  guardada miniatura-{n}@2x.png",
    "ERROR miniaturas::cuentas  la cuenta CLI-90{n} superó su cuota",
    "INFO  miniaturas::api  GET /v1/cuentas/55{n}/uso -> 200",
];

fn main() -> ExitCode {
    let primero = std::env::args().nth(1);
    if primero.as_deref() == Some(MODO_TRABAJADOR) {
        trabajar();
        return ExitCode::SUCCESS;
    }
    let direccion = primero.unwrap_or_else(|| "127.0.0.1:8750".into());
    match demostrar(&direccion) {
        Ok(()) => ExitCode::SUCCESS,
        Err(motivo) => {
            eprintln!("demo: {motivo}");
            ExitCode::FAILURE
        }
    }
}

/// El modo trabajador: atiende tareas hasta que se cierre su entrada. Para que la
/// demostración tenga movimiento, cada proceso se cae solo al cabo de 25 a 85 segundos.
fn trabajar() {
    let caida = Duration::from_secs(25 + u64::from(std::process::id() % 61));
    thread::spawn(move || {
        thread::sleep(caida);
        std::process::exit(3);
    });
    let _ = trabajador::atender(io::stdin().lock(), io::stdout().lock());
}

fn demostrar(texto: &str) -> Result<(), String> {
    let direccion = DireccionLocal::desde_texto(texto).map_err(|e| e.to_string())?;
    let ejecutable =
        std::env::current_exe().map_err(|e| format!("no encuentro este ejecutable: {e}"))?;
    let registro = ejecutable.with_file_name("ops-console-demo.log");
    escribir_registro_inicial(&registro)
        .map_err(|e| format!("no pude escribir {}: {e}", registro.display()))?;

    let reloj: Arc<dyn Reloj> = Arc::new(RelojReal::nuevo());
    let politica = Politica::nueva(2, 8, Duration::from_secs(150)).map_err(|e| e.to_string())?;
    let fabrica = FabricaDeProcesos::nueva(&ejecutable, vec![MODO_TRABAJADOR.into()]);
    let mut grupo =
        Grupo::nuevo(politica, Box::new(fabrica), Arc::clone(&reloj)).map_err(|e| e.to_string())?;
    grupo.escalar(4).map_err(|e| e.to_string())?;

    let consola = Arc::new(Consola::nueva(Piezas {
        grupo,
        vigia: Box::new(vigia_inventado()),
        servicios: [
            "miniaturas-api.service",
            "miniaturas-cola.service",
            "miniaturas-cdn-sync.timer",
            "miniaturas-limpieza.service",
            "miniaturas-exportador.service",
            "miniaturas-legado.service",
        ]
        .map(String::from)
        .to_vec(),
        registros: registro.clone(),
        entorno: Box::new(
            EntornoFijo::default()
                .con("MINIATURAS_CLAVE_API", "sk-demo-0000000000falsa")
                .con("MINIATURAS_TOKEN_CDN", "demo-token-falso")
                .con("MINIATURAS_URL_ORIGEN", "https://origen.example/imagenes/")
                .con("MINIATURAS_DIR_CACHE", "cache-de-demostracion"),
        ),
        reloj,
    }));
    let servidor = Servidor::escuchar(direccion, Limites::default(), consola.clone())
        .map_err(|e| format!("no se pudo abrir {direccion}: {e}"))?;
    println!("Demostración en http://{}/", servidor.direccion());
    println!("Registro inventado: {}", registro.display());
    println!("Ctrl-C para salir.");

    let revisor = Arc::clone(&consola);
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(3));
        revisor.tick();
    });
    thread::spawn(move || seguir_escribiendo(&registro));
    servidor.esperar();
    Ok(())
}

fn vigia_inventado() -> VigiaFijo {
    let activo = |pid, reinicios| EstadoServicio::Activo {
        subestado: "running".into(),
        pid: Some(pid),
        reinicios: Some(reinicios),
    };
    VigiaFijo::default()
        .con("miniaturas-api.service", activo(4242, 0))
        .con("miniaturas-cola.service", activo(4243, 2))
        .con(
            "miniaturas-cdn-sync.timer",
            EstadoServicio::Activo {
                subestado: "waiting".into(),
                pid: None,
                reinicios: None,
            },
        )
        .con(
            "miniaturas-limpieza.service",
            EstadoServicio::Inactivo {
                subestado: "dead".into(),
            },
        )
        .con(
            "miniaturas-exportador.service",
            EstadoServicio::Fallido {
                subestado: "failed".into(),
            },
        )
}

fn escribir_registro_inicial(ruta: &Path) -> io::Result<()> {
    let mut archivo = File::create(ruta)?;
    let ahora = segundos_unix();
    let total = u64::try_from(REGISTRO_INICIAL.len()).unwrap_or(0);
    for (i, linea) in (0u64..).zip(REGISTRO_INICIAL) {
        let marca = marca_de_tiempo(ahora.saturating_sub((total - i) * 7));
        // La línea siguiente a «token=» continúa el valor: va sin marca de tiempo.
        if linea.starts_with(' ') {
            writeln!(archivo, "{linea}")?;
        } else {
            writeln!(archivo, "{marca} {linea}")?;
        }
    }
    Ok(())
}

fn seguir_escribiendo(ruta: &Path) {
    let Ok(mut archivo) = OpenOptions::new().append(true).open(ruta) else {
        return;
    };
    for (n, plantilla) in (18_300u64..).zip(REGISTRO_EN_VIVO.iter().cycle()) {
        thread::sleep(Duration::from_secs(2));
        let linea = plantilla.replace("{n}", &n.to_string());
        if writeln!(archivo, "{} {linea}", marca_de_tiempo(segundos_unix())).is_err() {
            return;
        }
    }
}

fn segundos_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `AAAA-MM-DDTHH:MM:SSZ` en UTC, sin dependencias (días civiles de Howard Hinnant).
fn marca_de_tiempo(segundos: u64) -> String {
    let (dias, resto) = (segundos / 86_400, segundos % 86_400);
    let z = i64::try_from(dias).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let dia_de_era = z.rem_euclid(146_097);
    let anio_de_era =
        (dia_de_era - dia_de_era / 1460 + dia_de_era / 36_524 - dia_de_era / 146_096) / 365;
    let dia_del_anio = dia_de_era - (365 * anio_de_era + anio_de_era / 4 - anio_de_era / 100);
    let mes_marzo = (5 * dia_del_anio + 2) / 153;
    let dia = dia_del_anio - (153 * mes_marzo + 2) / 5 + 1;
    let mes = if mes_marzo < 10 {
        mes_marzo + 3
    } else {
        mes_marzo - 9
    };
    let anio = anio_de_era + era * 400 + i64::from(mes <= 2);
    format!(
        "{anio:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
}
