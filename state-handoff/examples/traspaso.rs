//! El controlador activo de una bomba le pasa su tabla de calibración al respaldo.
//!
//! ```text
//! cargo run --example traspaso
//! ```
//!
//! La tabla son ocho puntos (frecuencia del variador en Hz → caudal en L/min), 32 bytes. El
//! ejemplo recorre los casos: un traspaso en memoria, una repetición, un paquete que se
//! corrompe en el enlace y se retransmite, un aborto que no gasta la época, y un paquete para
//! otro nodo.

use state_handoff::paquete::sellar;
use state_handoff::receptor::Receptor;
use state_handoff::Error;

const ACTIVO: u16 = 1;
const RESPALDO: u16 = 2;

/// La tabla de calibración, en little-endian: pares (Hz, L/min).
fn tabla(ajuste: u16) -> [u8; 32] {
    let puntos: [(u16, u16); 8] = [
        (10, 40),
        (15, 75),
        (20, 112),
        (25, 150),
        (30, 186),
        (35, 221),
        (40, 255),
        (45, 287),
    ];
    let mut bytes = [0u8; 32];
    for (trozo, (hz, caudal)) in bytes.chunks_exact_mut(4).zip(puntos) {
        trozo[..2].copy_from_slice(&hz.to_le_bytes());
        trozo[2..].copy_from_slice(&(caudal + ajuste).to_le_bytes());
    }
    bytes
}

fn sellado(epoca: u64, destino: u16) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    let n = sellar(&tabla(epoca as u16), ACTIVO, destino, epoca, &mut b).expect("cabe");
    b.truncate(n);
    b
}

/// Prepara y confirma lo que haya cargado el respaldo, y cuenta qué pasó.
fn intentar(respaldo: &mut Receptor<128>, que: &str) {
    match respaldo.preparar() {
        Ok(preparado) => {
            let epoca = preparado.epoca();
            let primero = u16::from_le_bytes([preparado.contenido()[2], preparado.contenido()[3]]);
            preparado.confirmar();
            let limpia = respaldo.area_de_preparacion().iter().all(|&b| b == 0);
            println!(
                "{que}: confirmada la época {epoca} (10 Hz → {primero} L/min); área en ceros: {limpia}"
            );
        }
        Err(e) => println!("{que}: rechazado ({e})"),
    }
}

fn main() -> Result<(), Error> {
    let mut respaldo = Receptor::<128>::nuevo(RESPALDO);

    // 1. En memoria: el activo sella y el respaldo carga.
    let p41 = sellado(41, RESPALDO);
    println!("paquete de la época 41: {} bytes", p41.len());
    respaldo.cargar(&p41)?;
    intentar(&mut respaldo, "traspaso en memoria");

    // 2. El mismo paquete otra vez.
    respaldo.cargar(&p41)?;
    intentar(&mut respaldo, "repetición de la 41");

    // 3. Por el enlace, en tres trozos; en el camino se voltea un bit.
    let p42 = sellado(42, RESPALDO);
    let mut recibido = Vec::new();
    for trozo in p42.chunks(23) {
        recibido.extend_from_slice(trozo);
    }
    recibido[47] ^= 0x08;
    respaldo.cargar(&recibido)?;
    intentar(&mut respaldo, "la 42 con un bit volteado");

    // La retransmisión llega bien, pero la bomba está en una prueba y no puede tomar la
    // tabla todavía: se aborta, y la época no se gasta.
    respaldo.cargar(&p42)?;
    let preparado = respaldo.preparar()?;
    println!("la 42 retransmitida: preparada; la bomba está ocupada, se aborta");
    preparado.abortar();
    println!("última época confirmada: {}", respaldo.ultima_epoca());
    intentar(&mut respaldo, "la 42, cuando la bomba se libera");

    // 4. Un paquete para otro nodo.
    respaldo.cargar(&sellado(43, 7))?;
    intentar(&mut respaldo, "la 43 para el nodo 7");

    println!("última época confirmada: {}", respaldo.ultima_epoca());
    Ok(())
}
