//! Recorrido por las tres piezas, con el trabajador de demostración.
//!
//! ```text
//! cargo build --bins && cargo run --example demo
//! ```

use std::path::PathBuf;
use std::time::Duration;

use session_isolation::demo::{ConfigCompartida, Peticion, Respuesta, Tarea};
use session_isolation::{ejecutar, Evento, Juez, Limites, Medicion, Pool, Publicador, Receta, Umbrales};

/// El binario del trabajador vive junto al de este ejemplo.
fn trabajador(args: &[&str]) -> Receta {
    let mut ruta = std::env::current_exe().unwrap_or_default();
    ruta.pop();
    if ruta.ends_with("examples") {
        ruta.pop();
    }
    let ruta: PathBuf = ruta.join(format!("trabajador-demo{}", std::env::consts::EXE_SUFFIX));
    Receta::nueva(ruta, args)
}

fn main() {
    println!("1 · Instantánea compartida\n");
    let config = Publicador::nuevo(&ConfigCompartida { max_elementos: 4, factor: 2 });
    config.publicar(&ConfigCompartida { max_elementos: 4, factor: 10 });
    let foto = config.leer();
    println!("   generación {} · {:?}\n", foto.generacion, foto.valor);

    println!("2 · Sesiones efímeras (un proceso por petición)\n");
    let limites = Limites { tiempo: Duration::from_millis(500), max_bytes_salida: 4096 };
    let casos = [
        ("escalar [1, 2, 3]", Tarea::Escalar(vec![1, 2, 3])),
        ("colgarse 10 s", Tarea::Dormir { ms: 10_000 }),
        ("inundar 1 MiB", Tarea::Inundar { bytes: 1 << 20 }),
        ("demasiados elementos", Tarea::Escalar(vec![1; 9])),
    ];
    for (nombre, tarea) in casos {
        let p = Peticion { generacion: foto.generacion, config: foto.valor, tarea };
        match ejecutar::<_, Respuesta>(&trabajador(&[]), &p, &limites) {
            Ok(r) => println!("   {nombre:<22} → {:?} (generación {})", r.valores, r.generacion),
            Err(e) => println!("   {nombre:<22} → contenido: {e}"),
        }
    }

    println!("\n3 · Pool con reemplazo en caliente\n");
    let mut pool = Pool::nuevo(3, 1);
    for nombre in ["ingesta", "índices", "reportes"] {
        if let Err(e) = pool.lanzar(nombre, trabajador(&["residente"])) {
            println!("   no se pudo lanzar {nombre}: {e}");
            return;
        }
    }
    for s in pool.vista() {
        println!("   slot {} · {:<9} · pid {}", s.meta.id, s.meta.etiqueta, s.pid);
    }

    // Mediciones inventadas: el slot "índices" pierde memoria de forma sostenida.
    let pid_con_fuga = pool.vista()[1].pid;
    let mut juez = Juez::nuevo(Umbrales { persistencia: 3, ..Umbrales::default() });
    println!();
    for ciclo in 1..=3 {
        let eventos = pool.ciclo_de_salud(&mut juez, |pid| Medicion {
            memoria_bytes: Some(if pid == pid_con_fuga { 900 << 20 } else { 40 << 20 }),
            latencia: Some(Duration::from_millis(15)),
        });
        if eventos.is_empty() {
            println!("   ciclo {ciclo}: sin cambios (crítico, todavía sin persistencia suficiente)");
        }
        for e in eventos {
            if let Evento::Reemplazado { slot, pid_anterior, pid_nuevo, motivo } = e {
                println!("   ciclo {ciclo}: slot {slot} · pid {pid_anterior} → {pid_nuevo} · {motivo}");
            }
        }
    }

    println!();
    for s in pool.vista() {
        println!("   slot {} · {:<9} · pid {} · reemplazos {}", s.meta.id, s.meta.etiqueta, s.pid, s.meta.reemplazos);
    }
    println!("\n   al salir, el pool termina y espera a todos sus procesos");
}
