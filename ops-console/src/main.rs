//! `ops-console`: la consola de operaciones del servicio de miniaturas.
//!
//! ```text
//! ops-console --escuchar 127.0.0.1:8750 --trabajador <ruta> --registros <archivo> [--min N --max N]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ops_console::api::{Consola, Piezas};
use ops_console::direccion::DireccionLocal;
use ops_console::grupo::{FabricaDeProcesos, Grupo, Politica};
use ops_console::http::Limites;
use ops_console::reloj::{Reloj, RelojReal};
use ops_console::servidor::Servidor;
use ops_console::sistema::EntornoDelProceso;
use ops_console::vigia::{unidad_valida, VigiaSystemctl};

const USO: &str = "\
uso: ops-console --escuchar DIRECCIÓN --trabajador PROGRAMA --registros ARCHIVO [opciones]

  --escuchar DIRECCIÓN   dónde atender: 127.0.0.1:8750, [::1]:8750 o localhost:8750
                         (solo loopback: la consola no se expone a la red)
  --trabajador PROGRAMA  el programa que ocupa cada posición del grupo
  --registros ARCHIVO    el registro del servicio; se muestra redactado
  --min N                posiciones mínimas del grupo (por defecto 2)
  --max N                posiciones máximas del grupo (por defecto 8)
  --vida SEGUNDOS        cuánto vive un proceso antes de reciclarlo (por defecto 3600)
  --tick SEGUNDOS        cada cuánto se revisa el grupo (por defecto 5)
  --servicio UNIDAD      una unidad de systemd a vigilar; se puede repetir
";

struct Opciones {
    escuchar: DireccionLocal,
    trabajador: PathBuf,
    registros: PathBuf,
    min: usize,
    max: usize,
    vida: Duration,
    tick: Duration,
    servicios: Vec<String>,
}

fn leer_opciones(mut argumentos: impl Iterator<Item = String>) -> Result<Opciones, String> {
    let mut escuchar = None;
    let mut trabajador = None;
    let mut registros = None;
    let (mut min, mut max, mut vida, mut tick) = (2usize, 8usize, 3600u64, 5u64);
    let mut servicios = Vec::new();
    while let Some(opcion) = argumentos.next() {
        let mut valor = || {
            argumentos
                .next()
                .ok_or_else(|| format!("a {opcion} le falta su valor"))
        };
        let numero = |texto: String| {
            texto
                .parse::<u64>()
                .map_err(|_| format!("«{texto}» no es un número entero positivo"))
        };
        match opcion.as_str() {
            "--escuchar" => {
                let texto = valor()?;
                let direccion = DireccionLocal::desde_texto(&texto)
                    .map_err(|e| format!("no escucho en {texto}: {e}"))?;
                escuchar = Some(direccion);
            }
            "--trabajador" => trabajador = Some(PathBuf::from(valor()?)),
            "--registros" => registros = Some(PathBuf::from(valor()?)),
            "--min" => min = usize::try_from(numero(valor()?)?).unwrap_or(usize::MAX),
            "--max" => max = usize::try_from(numero(valor()?)?).unwrap_or(usize::MAX),
            "--vida" => vida = numero(valor()?)?,
            "--tick" => tick = numero(valor()?)?.max(1),
            "--servicio" => {
                let unidad = valor()?;
                if !unidad_valida(&unidad) {
                    return Err(format!("«{unidad}» no es un nombre de unidad válido"));
                }
                servicios.push(unidad);
            }
            "-h" | "--help" | "--ayuda" => return Err(String::new()),
            otra => return Err(format!("opción desconocida: {otra}")),
        }
    }
    Ok(Opciones {
        escuchar: escuchar.ok_or("falta --escuchar")?,
        trabajador: trabajador.ok_or("falta --trabajador")?,
        registros: registros.ok_or("falta --registros")?,
        min,
        max,
        vida: Duration::from_secs(vida),
        tick: Duration::from_secs(tick),
        servicios,
    })
}

fn main() -> ExitCode {
    let opciones = match leer_opciones(std::env::args().skip(1)) {
        Ok(opciones) => opciones,
        Err(motivo) if motivo.is_empty() => {
            print!("{USO}");
            return ExitCode::SUCCESS;
        }
        Err(motivo) => {
            eprintln!("ops-console: {motivo}\n\n{USO}");
            return ExitCode::from(2);
        }
    };
    match arrancar(opciones) {
        Ok(()) => ExitCode::SUCCESS,
        Err(motivo) => {
            eprintln!("ops-console: {motivo}");
            ExitCode::FAILURE
        }
    }
}

fn arrancar(o: Opciones) -> Result<(), String> {
    let reloj: Arc<dyn Reloj> = Arc::new(RelojReal::nuevo());
    let politica = Politica::nueva(o.min, o.max, o.vida).map_err(|e| e.to_string())?;
    let fabrica = FabricaDeProcesos::nueva(&o.trabajador, Vec::new());
    let grupo = Grupo::nuevo(politica, Box::new(fabrica), Arc::clone(&reloj))
        .map_err(|e| format!("{} ({})", e, o.trabajador.display()))?;
    let consola = Arc::new(Consola::nueva(Piezas {
        grupo,
        vigia: Box::new(VigiaSystemctl::nueva(Arc::clone(&reloj))),
        servicios: o.servicios,
        registros: o.registros,
        entorno: Box::new(EntornoDelProceso),
        reloj,
    }));
    let servidor = Servidor::escuchar(o.escuchar, Limites::default(), consola.clone())
        .map_err(|e| format!("no se pudo abrir {}: {e}", o.escuchar))?;
    println!("ops-console atiende en http://{}/", servidor.direccion());
    println!("Ctrl-C para salir: los trabajadores terminan solos al cerrarse su entrada.");

    let revisor = Arc::clone(&consola);
    let cada = o.tick;
    thread::Builder::new()
        .name("ops-console-tick".into())
        .spawn(move || loop {
            thread::sleep(cada);
            revisor.tick();
        })
        .map_err(|e| format!("no se pudo lanzar el tick: {e}"))?;
    servidor.esperar();
    Ok(())
}
