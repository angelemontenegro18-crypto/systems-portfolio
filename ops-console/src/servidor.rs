//! El servidor HTTP: escucha en una [`DireccionLocal`], atiende una petición por conexión y
//! pasa a la aplicación solo lo que superó todas las comprobaciones.
//!
//! El orden de cada conexión:
//!
//! 1. Leer la cabeza, con el plazo total y los límites de largo vigilados mientras llegan los
//!    bytes (408, 414, 431).
//! 2. Validarla: protocolo estricto y contrabando de peticiones (400, 405, 501, 505).
//! 3. `Host` del propio servidor (421) — antes que cualquier otra cosa de la aplicación.
//! 4. En un `POST`: las defensas contra CSRF (415, 403), `Content-Length` presente (411) y
//!    dentro del tope (413) — todo **antes** de leer un solo byte del cuerpo.
//! 5. Leer el cuerpo exacto y entregar la petición al [`Manejador`].
//!
//! Escuchar exige una `DireccionLocal`; un `SocketAddr` crudo no compila:
//!
//! ```compile_fail,E0308
//! use std::net::SocketAddr;
//! use std::sync::Arc;
//! use ops_console::http::{Limites, Peticion, Respuesta};
//! use ops_console::servidor::{Manejador, Servidor};
//!
//! struct Nada;
//! impl Manejador for Nada {
//!     fn atender(&self, _: &Peticion) -> Respuesta {
//!         Respuesta::error(404, "nada", "nada")
//!     }
//! }
//!
//! let abierta: SocketAddr = "0.0.0.0:8750".parse().unwrap();
//! // error[E0308]: se esperaba una `DireccionLocal`.
//! let _ = Servidor::escuchar(abierta, Limites::default(), Arc::new(Nada));
//! ```

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::defensas::{verificar_host, verificar_post};
use crate::direccion::DireccionLocal;
use crate::http::{
    analizar_cabeza, leer_cabeza, leer_cuerpo, Limites, Metodo, Peticion, Rechazo, Respuesta,
};

/// La aplicación: recibe peticiones ya validadas y devuelve respuestas.
pub trait Manejador: Send + Sync + 'static {
    /// Atiende una petición.
    fn atender(&self, peticion: &Peticion) -> Respuesta;
}

/// Un servidor escuchando. Al soltarlo deja de aceptar conexiones.
#[derive(Debug)]
pub struct Servidor {
    direccion: SocketAddr,
    parar: Arc<AtomicBool>,
    activas: Arc<AtomicUsize>,
    hilo: Option<JoinHandle<()>>,
}

impl Servidor {
    /// Abre el puerto y empieza a atender en un hilo propio. Con el puerto 0 el sistema elige
    /// uno libre: [`Servidor::direccion`] dice cuál.
    pub fn escuchar(
        direccion: DireccionLocal,
        limites: Limites,
        manejador: Arc<dyn Manejador>,
    ) -> io::Result<Servidor> {
        let oyente = TcpListener::bind(direccion.socket())?;
        let real = oyente.local_addr()?;
        let parar = Arc::new(AtomicBool::new(false));
        let activas = Arc::new(AtomicUsize::new(0));
        let hilo = {
            let parar = Arc::clone(&parar);
            let activas = Arc::clone(&activas);
            let limites = Arc::new(limites);
            thread::Builder::new()
                .name("ops-console-aceptar".into())
                .spawn(move || aceptar(oyente, real, limites, manejador, parar, activas))?
        };
        Ok(Servidor {
            direccion: real,
            parar,
            activas,
            hilo: Some(hilo),
        })
    }

    /// La dirección en la que escucha, con el puerto real.
    pub fn direccion(&self) -> SocketAddr {
        self.direccion
    }

    /// Conexiones que se están atendiendo ahora.
    pub fn conexiones_activas(&self) -> usize {
        self.activas.load(Ordering::SeqCst)
    }

    /// Bloquea hasta que el servidor se detenga (en la práctica, para siempre).
    pub fn esperar(mut self) {
        if let Some(hilo) = self.hilo.take() {
            let _ = hilo.join();
        }
    }
}

impl Drop for Servidor {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::SeqCst);
        // Una conexión propia despierta al `accept` bloqueado para que vea la señal.
        let _ = TcpStream::connect_timeout(&self.direccion, Duration::from_millis(500));
        if let Some(hilo) = self.hilo.take() {
            let _ = hilo.join();
        }
    }
}

/// Descuenta la conexión al terminar de atenderla, pase lo que pase.
struct Ocupada(Arc<AtomicUsize>);

impl Drop for Ocupada {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn aceptar(
    oyente: TcpListener,
    escucha: SocketAddr,
    limites: Arc<Limites>,
    manejador: Arc<dyn Manejador>,
    parar: Arc<AtomicBool>,
    activas: Arc<AtomicUsize>,
) {
    for flujo in oyente.incoming() {
        if parar.load(Ordering::SeqCst) {
            break;
        }
        let Ok(flujo) = flujo else {
            // Si `accept` falla seguido (sin descriptores libres, por ejemplo), una pausa breve
            // evita girar en vacío.
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        if activas.load(Ordering::SeqCst) >= limites.conexiones_max {
            rechazar_por_ocupado(flujo);
            continue;
        }
        activas.fetch_add(1, Ordering::SeqCst);
        let ocupada = Ocupada(Arc::clone(&activas));
        let limites = Arc::clone(&limites);
        let manejador = Arc::clone(&manejador);
        let lanzado = thread::Builder::new()
            .name("ops-console-conexion".into())
            .spawn(move || {
                let _ocupada = ocupada;
                atender_conexion(flujo, escucha, &limites, manejador.as_ref());
            });
        // Si no se pudo lanzar el hilo, el cierre ya devolvió la conexión al soltar `ocupada`.
        drop(lanzado);
    }
}

/// 503 inmediato desde el hilo que acepta, sin lanzar otro: el tope de conexiones no se
/// puede saltar abriendo más. El cierre espera apenas [`CALMA_AL_RECHAZAR`].
fn rechazar_por_ocupado(mut flujo: TcpStream) {
    let respuesta = Respuesta::error(
        503,
        "servidor_ocupado",
        "hay demasiadas conexiones abiertas; reintenta en unos segundos",
    )
    .con_cabecera("Retry-After", "2");
    let _ = flujo.set_write_timeout(Some(Duration::from_millis(200)));
    let _ = flujo.write_all(&respuesta.serializar());
    cerrar_con_calma(flujo, CALMA_AL_RECHAZAR);
}

fn atender_conexion(
    mut flujo: TcpStream,
    escucha: SocketAddr,
    limites: &Limites,
    manejador: &dyn Manejador,
) {
    let plazo = Instant::now() + limites.tiempo_lectura;
    let _ = flujo.set_write_timeout(Some(limites.tiempo_escritura));
    let respuesta = match recibir(&mut flujo, escucha, limites, plazo) {
        Ok(peticion) => manejador.atender(&peticion),
        Err(rechazo) => rechazo.respuesta(),
    };
    let _ = flujo.write_all(&respuesta.serializar());
    cerrar_con_calma(flujo, CALMA);
}

fn recibir(
    flujo: &mut TcpStream,
    escucha: SocketAddr,
    limites: &Limites,
    plazo: Instant,
) -> Result<Peticion, Rechazo> {
    let (bytes, sobrante) = leer_cabeza(flujo, limites, plazo)?;
    let cabeza = analizar_cabeza(&bytes, limites)?;
    verificar_host(&cabeza, escucha)?;
    let longitud = match cabeza.metodo {
        Metodo::Get => {
            if cabeza.longitud.unwrap_or(0) > 0 {
                return Err(Rechazo::nuevo(
                    400,
                    "get_con_cuerpo",
                    "un GET no lleva cuerpo",
                ));
            }
            0
        }
        Metodo::Post => {
            verificar_post(&cabeza)?;
            let longitud = cabeza.longitud.ok_or_else(|| {
                Rechazo::nuevo(411, "falta_content_length", "un POST exige Content-Length")
            })?;
            if longitud > limites.cuerpo_max {
                return Err(Rechazo::nuevo(
                    413,
                    "cuerpo_demasiado_grande",
                    format!("el cuerpo supera {} bytes", limites.cuerpo_max),
                ));
            }
            longitud
        }
    };
    let cuerpo = leer_cuerpo(flujo, longitud, sobrante, plazo)?;
    Ok(Peticion { cabeza, cuerpo })
}

/// Lo más que se espera, después de responder, a que el cliente deje de mandar.
const CALMA: Duration = Duration::from_millis(200);

/// Lo mismo al rechazar por exceso de conexiones, que ocurre en el hilo que acepta.
const CALMA_AL_RECHAZAR: Duration = Duration::from_millis(50);

/// Lo más que se descarta al cerrar.
const DESCARTE_MAX: usize = 64 * 1024;

/// Cierra sin perder la respuesta: deja de escribir, descarta lo que el cliente aún mande
/// —hasta [`DESCARTE_MAX`] bytes y durante `calma` **en total**— y recién entonces cierra.
/// Cerrar con datos sin leer haría que el sistema enviara un RST, y el cliente podría perder
/// la respuesta antes de leerla. El plazo es total para que un cliente que sigue goteando no
/// retenga su lugar de conexión.
fn cerrar_con_calma(mut flujo: TcpStream, calma: Duration) {
    let _ = flujo.shutdown(Shutdown::Write);
    let plazo = Instant::now() + calma;
    let mut descarte = [0u8; 4096];
    let mut total = 0usize;
    while total < DESCARTE_MAX {
        let resta = plazo.saturating_duration_since(Instant::now());
        if resta.is_zero() || flujo.set_read_timeout(Some(resta)).is_err() {
            break;
        }
        match flujo.read(&mut descarte) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
}
