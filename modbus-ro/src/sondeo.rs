//! Sondeo en segundo plano con caché.
//!
//! Un panel no puede quedarse colgado porque un equipo tarda en contestar.
//! [`Sondeador`] lee los equipos desde su propio hilo y deja el último
//! resultado en un caché; quien consulta [`Sondeador::lecturas`] obtiene una
//! copia al instante. El lock del caché **nunca** se toma durante una
//! operación de red: el hilo arma la lectura completa y recién entonces la
//! publica.

use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::cliente::{Cliente, ErrorModbus};
use crate::trama::{ErrorTrama, Funcion};

/// Un rango de registros a leer en cada ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloque {
    /// Nombre para mostrar (p. ej. "temperatura de la cámara 2").
    pub etiqueta: String,
    /// Qué tipo de registro.
    pub funcion: Funcion,
    /// Primer registro.
    pub direccion: u16,
    /// Cuántos registros.
    pub cantidad: u16,
}

/// Un equipo a sondear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equipo {
    /// Nombre para mostrar.
    pub nombre: String,
    /// Dirección TCP.
    pub direccion: SocketAddr,
    /// Unidad Modbus.
    pub unidad: u8,
    /// Qué se lee en cada ciclo.
    pub bloques: Vec<Bloque>,
}

/// Estado de un equipo según su último ciclo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoEquipo {
    /// Todavía no terminó el primer ciclo.
    Sondeando,
    /// Todos los bloques se leyeron bien.
    EnLinea,
    /// Algunos bloques se leyeron y otros no.
    Degradado,
    /// No se pudo leer ningún bloque (o ni siquiera conectar).
    FueraDeLinea,
}

/// Resultado de un bloque en el último ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoBloque {
    /// Etiqueta del bloque.
    pub etiqueta: String,
    /// Los registros, o el motivo por el que no se pudieron leer.
    pub valores: Result<Vec<u16>, String>,
}

/// Última lectura conocida de un equipo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lectura {
    /// Nombre del equipo.
    pub equipo: String,
    /// Estado resumido.
    pub estado: EstadoEquipo,
    /// Un resultado por bloque, en el orden configurado.
    pub bloques: Vec<ResultadoBloque>,
    /// Cuándo terminó el ciclo que produjo esta lectura.
    pub tomada: Option<Instant>,
    /// Falla de conexión, si no se llegó a leer ningún bloque.
    pub error: Option<String>,
}

impl Lectura {
    fn inicial(equipo: &Equipo) -> Self {
        Self {
            equipo: equipo.nombre.clone(),
            estado: EstadoEquipo::Sondeando,
            bloques: Vec::new(),
            tomada: None,
            error: None,
        }
    }

    /// Cuánto hace que se tomó. `None` si todavía no hubo ningún ciclo.
    pub fn antiguedad(&self, ahora: Instant) -> Option<Duration> {
        self.tomada.map(|t| ahora.saturating_duration_since(t))
    }
}

/// Lee equipos en segundo plano y sirve la última lectura sin esperar a la red.
pub struct Sondeador {
    cache: Arc<Mutex<Vec<Lectura>>>,
    parar: Option<mpsc::Sender<()>>,
    hilo: Option<JoinHandle<()>>,
}

impl Sondeador {
    /// Arranca el hilo de sondeo. El primer ciclo empieza de inmediato.
    pub fn iniciar(
        equipos: Vec<Equipo>,
        intervalo: Duration,
        tiempo_conexion: Duration,
        tiempo_peticion: Duration,
    ) -> Self {
        let cache = Arc::new(Mutex::new(
            equipos.iter().map(Lectura::inicial).collect::<Vec<_>>(),
        ));
        let (parar, senal) = mpsc::channel::<()>();
        let cache_hilo = Arc::clone(&cache);

        let hilo = thread::spawn(move || {
            let mut conexiones: Vec<Option<Cliente<TcpStream>>> =
                equipos.iter().map(|_| None).collect();
            loop {
                for (i, equipo) in equipos.iter().enumerate() {
                    if debe_parar(&senal) {
                        return;
                    }
                    let lectura =
                        leer_equipo(equipo, &mut conexiones[i], tiempo_conexion, tiempo_peticion);
                    // El lock se toma solo para reemplazar la entrada: la red ya terminó.
                    if let Some(entrada) = bloquear(&cache_hilo).get_mut(i) {
                        *entrada = lectura;
                    }
                }
                match senal.recv_timeout(intervalo) {
                    Err(RecvTimeoutError::Timeout) => {}
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });

        Self {
            cache,
            parar: Some(parar),
            hilo: Some(hilo),
        }
    }

    /// Copia de la última lectura de cada equipo, en el orden configurado.
    /// No espera a la red: si un equipo está tardando, devuelve lo anterior.
    pub fn lecturas(&self) -> Vec<Lectura> {
        bloquear(&self.cache).clone()
    }

    /// Detiene el hilo y espera a que termine. Si hay una lectura en curso, la
    /// espera como mucho hasta que venzan sus timeouts de red.
    pub fn detener(mut self) {
        self.cerrar();
    }

    fn cerrar(&mut self) {
        if let Some(parar) = self.parar.take() {
            // Si el hilo ya terminó, el canal está cerrado: no hay nada que avisar.
            let _ = parar.send(());
        }
        if let Some(hilo) = self.hilo.take() {
            let _ = hilo.join();
        }
    }
}

impl Drop for Sondeador {
    fn drop(&mut self) {
        self.cerrar();
    }
}

/// Un panic en otro hilo no debe dejar el caché inservible: los datos siguen
/// siendo una lista de lecturas completas, así que se recupera el guardia.
fn bloquear(cache: &Mutex<Vec<Lectura>>) -> MutexGuard<'_, Vec<Lectura>> {
    cache
        .lock()
        .unwrap_or_else(|envenenado| envenenado.into_inner())
}

fn debe_parar(senal: &mpsc::Receiver<()>) -> bool {
    !matches!(senal.try_recv(), Err(mpsc::TryRecvError::Empty))
}

/// Tras una falla de red o una trama que no cuadra, el flujo puede quedar
/// desincronizado (bytes de una respuesta vieja esperando). Solo una excepción
/// Modbus deja la conexión en un estado conocido.
fn conexion_sigue_sana(error: &ErrorModbus) -> bool {
    matches!(
        error,
        ErrorModbus::Trama(ErrorTrama::Excepcion { .. }) | ErrorModbus::PeticionInvalida(_)
    )
}

fn leer_equipo(
    equipo: &Equipo,
    conexion: &mut Option<Cliente<TcpStream>>,
    tiempo_conexion: Duration,
    tiempo_peticion: Duration,
) -> Lectura {
    if conexion.is_none() {
        match Cliente::conectar(
            equipo.direccion,
            equipo.unidad,
            tiempo_conexion,
            tiempo_peticion,
        ) {
            Ok(c) => *conexion = Some(c),
            Err(e) => {
                return Lectura {
                    equipo: equipo.nombre.clone(),
                    estado: EstadoEquipo::FueraDeLinea,
                    bloques: Vec::new(),
                    tomada: Some(Instant::now()),
                    error: Some(e.to_string()),
                };
            }
        }
    }

    let mut bloques = Vec::with_capacity(equipo.bloques.len());
    for bloque in &equipo.bloques {
        let valores = match conexion.as_mut() {
            Some(cliente) => {
                match cliente.leer(bloque.funcion, bloque.direccion, bloque.cantidad) {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        if !conexion_sigue_sana(&e) {
                            *conexion = None;
                        }
                        Err(e.to_string())
                    }
                }
            }
            None => Err("conexión cerrada tras una falla anterior en este ciclo".to_string()),
        };
        bloques.push(ResultadoBloque {
            etiqueta: bloque.etiqueta.clone(),
            valores,
        });
    }

    // Se compara primero contra el total: un equipo sin bloques configurados
    // que conectó bien está en línea, no fuera de línea.
    let buenos = bloques.iter().filter(|b| b.valores.is_ok()).count();
    let estado = if buenos == bloques.len() {
        EstadoEquipo::EnLinea
    } else if buenos == 0 {
        EstadoEquipo::FueraDeLinea
    } else {
        EstadoEquipo::Degradado
    };

    Lectura {
        equipo: equipo.nombre.clone(),
        estado,
        bloques,
        tomada: Some(Instant::now()),
        error: None,
    }
}
