//! Sesiones efímeras: un proceso por petición.
//!
//! La petición viaja por **stdin** como JSON y la respuesta vuelve por
//! **stdout**. El proceso hijo solo ve lo que se le manda — nunca la memoria
//! del coordinador — y muere al terminar.
//!
//! Tres cosas que un proceso hijo puede hacer mal, y cómo se contienen:
//!
//! - **Colgarse**: hay un plazo total. Al vencer, se lo mata y se lo espera.
//! - **Inundar la salida**: hay un tope de bytes. Al pasarlo, se lo mata.
//! - **No leer su entrada**: stdin se escribe desde otro hilo, así que un hijo
//!   que no lee no puede bloquear al coordinador.
//!
//! En todos los caminos, incluidos los de error, el hijo se **espera**
//! (`wait`) después de matarlo: no quedan procesos zombi.

use std::fmt;
use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::Receta;

/// Cuánto se le permite a una sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    /// Plazo total, desde que se lanza el proceso hasta que termina.
    pub tiempo: Duration,
    /// Máximo de bytes que puede escribir en stdout.
    pub max_bytes_salida: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self { tiempo: Duration::from_secs(5), max_bytes_salida: 1 << 20 }
    }
}

/// Cómo puede terminar mal una sesión.
#[derive(Debug)]
pub enum ErrorIpc {
    /// La petición no se pudo serializar.
    Peticion(serde_json::Error),
    /// El proceso no se pudo lanzar.
    Lanzar(io::Error),
    /// Falla leyendo la salida del proceso.
    Leer(io::Error),
    /// Se venció el plazo. El proceso ya fue terminado y esperado.
    Timeout {
        /// El plazo que se venció.
        tiempo: Duration,
        /// El pid que tenía el proceso.
        pid: u32,
    },
    /// El proceso escribió más de lo permitido. Ya fue terminado y esperado.
    SalidaExcedida {
        /// El tope.
        limite: usize,
    },
    /// El proceso terminó con error.
    Termino {
        /// Código de salida, si lo hubo (no lo hay si lo mató una señal).
        codigo: Option<i32>,
        /// Lo que escribió en stderr, recortado.
        stderr: String,
    },
    /// La respuesta no era el JSON esperado.
    Respuesta(serde_json::Error),
}

impl fmt::Display for ErrorIpc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Peticion(e) => write!(f, "no se pudo serializar la petición: {e}"),
            Self::Lanzar(e) => write!(f, "no se pudo lanzar el proceso: {e}"),
            Self::Leer(e) => write!(f, "falla leyendo la salida: {e}"),
            Self::Timeout { tiempo, pid } => write!(f, "el proceso {pid} no terminó en {tiempo:?}"),
            Self::SalidaExcedida { limite } => write!(f, "el proceso escribió más de {limite} bytes"),
            Self::Termino { codigo, stderr } => {
                match codigo {
                    Some(c) => write!(f, "el proceso terminó con código {c}")?,
                    None => write!(f, "el proceso fue terminado por una señal")?,
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Self::Respuesta(e) => write!(f, "respuesta inválida: {e}"),
        }
    }
}

impl std::error::Error for ErrorIpc {}

/// Bytes de stderr que se conservan para el diagnóstico.
const MAX_STDERR: usize = 4 * 1024;

enum FallaLectura {
    Excedida,
    Io(io::Error),
}

/// Lanza un proceso, le entrega `peticion`, y devuelve su respuesta.
pub fn ejecutar<P, R>(receta: &Receta, peticion: &P, limites: &Limites) -> Result<R, ErrorIpc>
where
    P: Serialize,
    R: DeserializeOwned,
{
    let mut entrada = serde_json::to_vec(peticion).map_err(ErrorIpc::Peticion)?;
    entrada.push(b'\n');

    let mut hijo = Command::new(&receta.programa)
        .args(&receta.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ErrorIpc::Lanzar)?;
    let pid = hijo.id();
    let vence = Instant::now() + limites.tiempo;

    // Cada tubería en su propio hilo: ninguna puede bloquear a las demás.
    if let Some(mut stdin) = hijo.stdin.take() {
        thread::spawn(move || {
            // Si el hijo muere sin leer, la escritura falla; no hay nada que hacer.
            let _ = stdin.write_all(&entrada);
        });
    }
    let (enviar_stdout, recibir_stdout) = mpsc::channel();
    if let Some(stdout) = hijo.stdout.take() {
        let tope = limites.max_bytes_salida;
        thread::spawn(move || {
            let _ = enviar_stdout.send(leer_con_tope(stdout, tope));
        });
    }
    let lector_stderr = hijo.stderr.take().map(|stderr| thread::spawn(move || leer_recortado(stderr, MAX_STDERR)));

    let salida = match recibir_stdout.recv_timeout(vence.saturating_duration_since(Instant::now())) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(FallaLectura::Excedida)) => {
            terminar(&mut hijo);
            return Err(ErrorIpc::SalidaExcedida { limite: limites.max_bytes_salida });
        }
        Ok(Err(FallaLectura::Io(e))) => {
            terminar(&mut hijo);
            return Err(ErrorIpc::Leer(e));
        }
        Err(_) => {
            terminar(&mut hijo);
            return Err(ErrorIpc::Timeout { tiempo: limites.tiempo, pid });
        }
    };

    // stdout se cerró, pero un proceso puede cerrar su salida y seguir vivo:
    // se le da lo que quede del plazo para terminar.
    let Some(estado) = esperar_hasta(&mut hijo, vence) else {
        terminar(&mut hijo);
        return Err(ErrorIpc::Timeout { tiempo: limites.tiempo, pid });
    };

    if !estado.success() {
        let stderr = lector_stderr.and_then(|h| h.join().ok()).unwrap_or_default();
        return Err(ErrorIpc::Termino { codigo: estado.code(), stderr });
    }
    serde_json::from_slice(&salida).map_err(ErrorIpc::Respuesta)
}

/// Mata al proceso y lo espera. Si ya había terminado, `kill` falla y `wait`
/// igual lo cosecha.
pub(crate) fn terminar(hijo: &mut Child) {
    let _ = hijo.kill();
    let _ = hijo.wait();
}

fn esperar_hasta(hijo: &mut Child, vence: Instant) -> Option<ExitStatus> {
    loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => return Some(estado),
            Ok(None) if Instant::now() < vence => thread::sleep(Duration::from_millis(2)),
            Ok(None) | Err(_) => return None,
        }
    }
}

fn leer_con_tope(mut r: impl Read, tope: usize) -> Result<Vec<u8>, FallaLectura> {
    let mut acumulado = Vec::new();
    let mut trozo = [0u8; 8 * 1024];
    loop {
        match r.read(&mut trozo) {
            Ok(0) => return Ok(acumulado),
            Ok(n) if acumulado.len() + n > tope => return Err(FallaLectura::Excedida),
            Ok(n) => acumulado.extend_from_slice(&trozo[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(FallaLectura::Io(e)),
        }
    }
}

/// Lee todo (para que el hijo nunca se bloquee escribiendo) pero conserva
/// solo los primeros `tope` bytes.
fn leer_recortado(mut r: impl Read, tope: usize) -> String {
    let mut conservado = Vec::new();
    let mut trozo = [0u8; 4 * 1024];
    while let Ok(n) = r.read(&mut trozo) {
        if n == 0 {
            break;
        }
        let cabe = tope.saturating_sub(conservado.len()).min(n);
        conservado.extend_from_slice(&trozo[..cabe]);
    }
    String::from_utf8_lossy(&conservado).trim().to_string()
}
