//! Trabajador de demostración.
//!
//! - Sin argumentos: lee **una** petición JSON de stdin, responde por stdout y termina.
//! - `residente`: se queda vivo hasta que se cierre su stdin (lo usa el pool).

use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use session_isolation::demo::{Peticion, Respuesta, Tarea};

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("residente") {
        // Drena stdin hasta que el pool lo cierre.
        let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
        return ExitCode::SUCCESS;
    }

    let mut linea = String::new();
    if let Err(e) = io::stdin().lock().read_line(&mut linea) {
        eprintln!("no se pudo leer la petición: {e}");
        return ExitCode::from(2);
    }
    let peticion: Peticion = match serde_json::from_str(&linea) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("petición inválida: {e}");
            return ExitCode::from(2);
        }
    };

    match peticion.tarea {
        Tarea::Escalar(valores) => {
            if valores.len() > peticion.config.max_elementos as usize {
                eprintln!(
                    "{} elementos, el máximo es {}",
                    valores.len(),
                    peticion.config.max_elementos
                );
                return ExitCode::from(3);
            }
            let valores = valores
                .iter()
                .map(|v| v.saturating_mul(peticion.config.factor))
                .collect();
            let respuesta = Respuesta {
                generacion: peticion.generacion,
                valores,
            };
            match serde_json::to_writer(io::stdout().lock(), &respuesta) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("no se pudo escribir la respuesta: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Tarea::Dormir { ms } => {
            thread::sleep(Duration::from_millis(ms));
            ExitCode::SUCCESS
        }
        Tarea::Inundar { bytes } => {
            let bloque = [b'x'; 8192];
            let mut salida = io::stdout().lock();
            let mut restantes = bytes;
            while restantes > 0 {
                let n = restantes.min(bloque.len());
                if salida.write_all(&bloque[..n]).is_err() {
                    break;
                }
                restantes -= n;
            }
            ExitCode::SUCCESS
        }
        Tarea::Fallar { codigo } => {
            eprintln!("falla pedida por la prueba");
            ExitCode::from(u8::try_from(codigo).unwrap_or(1))
        }
    }
}
