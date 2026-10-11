//! Lo que comparten las pruebas: carpetas propias, un cliente HTTP crudo, una fábrica de
//! procesos de mentira y la comprobación de si un proceso real sigue existiendo.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ops_console::grupo::{Fabrica, Proceso};
use ops_console::json::{self, Json};

// ─── Carpetas propias ─────────────────────────────────────────────────────────────────────

static CONTADOR: AtomicU32 = AtomicU32::new(0);

/// Una carpeta propia dentro del directorio que Cargo reserva para las pruebas de
/// integración (`CARGO_TARGET_TMPDIR`, dentro del directorio de compilación). Se borra al
/// soltarla, aunque la prueba falle.
pub struct Carpeta {
    ruta: PathBuf,
}

impl Carpeta {
    pub fn nueva(nombre: &str) -> Carpeta {
        let n = CONTADOR.fetch_add(1, Ordering::SeqCst);
        let ruta = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("ops-console-{}-{nombre}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ruta);
        std::fs::create_dir_all(&ruta).expect("crear la carpeta de la prueba");
        Carpeta { ruta }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// Escribe `contenido` en un archivo de la carpeta y devuelve su ruta.
    pub fn archivo(&self, nombre: &str, contenido: &[u8]) -> PathBuf {
        let ruta = self.ruta.join(nombre);
        std::fs::write(&ruta, contenido).expect("escribir el archivo de la prueba");
        ruta
    }
}

impl Drop for Carpeta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ruta);
    }
}

// ─── Cliente HTTP crudo ───────────────────────────────────────────────────────────────────

/// Una respuesta HTTP, leída byte a byte por la prueba.
#[derive(Debug, Clone)]
pub struct Respuesta {
    pub estado: u16,
    /// Nombres en minúsculas, en el orden en que llegaron.
    pub cabeceras: Vec<(String, String)>,
    pub cuerpo: Vec<u8>,
}

impl Respuesta {
    pub fn cabecera(&self, nombre: &str) -> Option<&str> {
        self.cabeceras
            .iter()
            .find(|(n, _)| n == nombre)
            .map(|(_, v)| v.as_str())
    }

    pub fn veces(&self, nombre: &str) -> usize {
        self.cabeceras.iter().filter(|(n, _)| n == nombre).count()
    }

    pub fn texto(&self) -> String {
        String::from_utf8_lossy(&self.cuerpo).into_owned()
    }

    pub fn json(&self) -> Json {
        json::analizar(&self.texto()).expect("el cuerpo es JSON")
    }

    /// El código de error de una respuesta `{"ok": false, "error": {…}}`.
    pub fn codigo(&self) -> String {
        match self.json().campo("error").and_then(|e| e.campo("codigo")) {
            Some(Json::Texto(codigo)) => codigo.clone(),
            otro => panic!("no hay código de error: {otro:?} en {}", self.texto()),
        }
    }
}

/// Analiza una respuesta completa.
pub fn analizar_respuesta(bytes: &[u8]) -> Respuesta {
    let fin = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("respuesta sin cabeza: {:?}", String::from_utf8_lossy(bytes)));
    let cabeza = std::str::from_utf8(&bytes[..fin]).expect("cabeza UTF-8");
    let mut lineas = cabeza.split("\r\n");
    let estado = lineas
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|e| e.parse().ok())
        .expect("línea de estado");
    let cabeceras = lineas
        .map(|l| {
            let (nombre, valor) = l.split_once(": ").expect("cabecera «nombre: valor»");
            (nombre.to_ascii_lowercase(), valor.to_string())
        })
        .collect();
    Respuesta {
        estado,
        cabeceras,
        cuerpo: bytes[fin + 4..].to_vec(),
    }
}

/// Lee hasta que el servidor cierra (una petición por conexión).
pub fn leer_respuesta(flujo: &mut TcpStream) -> Respuesta {
    flujo
        .set_read_timeout(Some(Duration::from_secs(15)))
        .expect("plazo de lectura");
    let mut bytes = Vec::new();
    let mut trozo = [0u8; 8192];
    loop {
        match flujo.read(&mut trozo) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&trozo[..n]),
            // Un reinicio después de la respuesta completa no importa: ya está leída.
            Err(e) if !bytes.is_empty() && e.kind() != io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("no llegó respuesta: {e}"),
        }
    }
    analizar_respuesta(&bytes)
}

/// Manda `peticion` tal cual y devuelve la respuesta.
pub fn crudo(direccion: SocketAddr, peticion: &[u8]) -> Respuesta {
    let mut flujo = TcpStream::connect(direccion).expect("conectar");
    // Si el servidor rechaza a mitad de camino puede cerrar antes de que terminemos de
    // escribir: lo que importa es la respuesta.
    let _ = flujo.write_all(peticion);
    leer_respuesta(&mut flujo)
}

/// Un `GET` correcto, con el `Host` del servidor.
pub fn get(direccion: SocketAddr, ruta: &str) -> Respuesta {
    crudo(
        direccion,
        format!("GET {ruta} HTTP/1.1\r\nHost: {direccion}\r\n\r\n").as_bytes(),
    )
}

/// Un `POST` correcto: JSON, la cabecera propia y el mismo origen.
pub fn post(direccion: SocketAddr, ruta: &str, cuerpo: &str) -> Respuesta {
    crudo(
        direccion,
        format!(
            "POST {ruta} HTTP/1.1\r\nHost: {direccion}\r\nContent-Type: application/json\r\n\
             X-Consola: 1\r\nOrigin: http://{direccion}\r\nContent-Length: {}\r\n\r\n{cuerpo}",
            cuerpo.len()
        )
        .as_bytes(),
    )
}

// ─── Procesos de mentira ──────────────────────────────────────────────────────────────────

/// Lo que se sabe de los procesos de mentira.
#[derive(Debug, Default)]
pub struct Registro {
    /// pid → si está vivo.
    pub vivos: HashMap<u32, bool>,
    /// Los pids lanzados, en orden.
    pub lanzados: Vec<u32>,
    /// Los pids terminados (y esperados) por el grupo, en orden.
    pub terminados: Vec<u32>,
    /// Lanzamientos y terminaciones, en el orden en que ocurrieron: `('L', pid)` o
    /// `('T', pid)`.
    pub eventos: Vec<(char, u32)>,
    /// Cuántos de los próximos lanzamientos fallan.
    pub fallos_pendientes: u32,
    /// Si se fija, los lanzamientos fallan una vez que se lanzaron tantos.
    pub tope_de_lanzamientos: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Procesos(Arc<Mutex<Registro>>);

impl Procesos {
    pub fn registro(&self) -> MutexGuard<'_, Registro> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Una fábrica que lanza procesos de mentira anotados aquí.
    pub fn fabrica(&self) -> Box<dyn Fabrica> {
        Box::new(FabricaDeMentira(self.clone()))
    }

    /// Simula que el proceso se cayó solo.
    pub fn matar(&self, pid: u32) {
        self.registro().vivos.insert(pid, false);
    }

    /// Hace fallar los próximos `n` lanzamientos.
    pub fn fallar(&self, n: u32) {
        self.registro().fallos_pendientes = n;
    }

    /// Deja lanzar solo `n` procesos en total; los siguientes fallan.
    pub fn permitir_solo(&self, n: usize) {
        self.registro().tope_de_lanzamientos = Some(n);
    }

    pub fn eventos(&self) -> Vec<(char, u32)> {
        self.registro().eventos.clone()
    }

    pub fn lanzados(&self) -> Vec<u32> {
        self.registro().lanzados.clone()
    }

    pub fn terminados(&self) -> Vec<u32> {
        self.registro().terminados.clone()
    }

    /// Los lanzados que el grupo no terminó.
    pub fn sin_terminar(&self) -> Vec<u32> {
        let r = self.registro();
        r.lanzados
            .iter()
            .copied()
            .filter(|pid| !r.terminados.contains(pid))
            .collect()
    }
}

struct FabricaDeMentira(Procesos);

impl Fabrica for FabricaDeMentira {
    fn lanzar(&mut self) -> io::Result<Box<dyn Proceso>> {
        let mut r = self.0.registro();
        let agotado = r
            .tope_de_lanzamientos
            .is_some_and(|tope| r.lanzados.len() >= tope);
        if r.fallos_pendientes > 0 || agotado {
            r.fallos_pendientes = r.fallos_pendientes.saturating_sub(1);
            return Err(io::Error::other("lanzamiento fallido a propósito"));
        }
        let pid = 1000 + u32::try_from(r.lanzados.len()).expect("pocos procesos");
        r.vivos.insert(pid, true);
        r.lanzados.push(pid);
        r.eventos.push(('L', pid));
        Ok(Box::new(ProcesoDeMentira {
            pid,
            procesos: self.0.clone(),
        }))
    }
}

/// Sin `Drop` a propósito: si el grupo suelta un proceso sin terminarlo, queda a la vista.
struct ProcesoDeMentira {
    pid: u32,
    procesos: Procesos,
}

impl Proceso for ProcesoDeMentira {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn sigue_vivo(&mut self) -> bool {
        self.procesos
            .registro()
            .vivos
            .get(&self.pid)
            .copied()
            .unwrap_or(false)
    }

    fn terminar(&mut self) {
        let mut r = self.procesos.registro();
        r.vivos.insert(self.pid, false);
        if !r.terminados.contains(&self.pid) {
            r.terminados.push(self.pid);
            r.eventos.push(('T', self.pid));
        }
    }
}

// ─── Procesos reales ──────────────────────────────────────────────────────────────────────

/// Si el proceso `pid` todavía existe. En Unix, `kill -0` también ve a un zombi: un hijo
/// terminado pero no esperado cuenta como existente.
#[allow(clippy::let_and_return)] // en cada sistema queda una sola de las dos ramas
pub fn existe_proceso(pid: u32) -> bool {
    #[cfg(unix)]
    let existe = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .expect("ejecutar kill -0")
        .status
        .success();
    #[cfg(windows)]
    let existe = {
        let salida = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .expect("ejecutar tasklist");
        String::from_utf8_lossy(&salida.stdout).contains(&format!("\"{pid}\""))
    };
    existe
}

/// Termina desde afuera un proceso real, como si se hubiera caído.
pub fn matar_desde_afuera(pid: u32) {
    #[cfg(unix)]
    let estado = Command::new("kill").arg(pid.to_string()).status();
    #[cfg(windows)]
    let estado = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status();
    assert!(
        estado.expect("ejecutar kill").success(),
        "no se pudo matar {pid}"
    );
}

/// Espera, como mucho `limite`, a que `condicion` se cumpla.
pub fn esperar_hasta(limite: Duration, mut condicion: impl FnMut() -> bool) -> bool {
    let fin = std::time::Instant::now() + limite;
    while std::time::Instant::now() < fin {
        if condicion() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    condicion()
}
