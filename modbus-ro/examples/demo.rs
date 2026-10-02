//! Demo autocontenida: levanta un equipo Modbus simulado en este mismo proceso,
//! lo lee con el cliente y después lo deja en manos del sondeador.
//!
//! ```text
//! cargo run --example demo
//! ```

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::Duration;

use modbus_ro::{Bloque, Cliente, Equipo, Funcion, Sondeador};

/// Un equipo de juguete: la "temperatura" sube un poco en cada lectura.
fn equipo_simulado() -> std::io::Result<SocketAddr> {
    let oyente = TcpListener::bind("127.0.0.1:0")?;
    let direccion = oyente.local_addr()?;
    thread::spawn(move || {
        for mut flujo in oyente.incoming().flatten() {
            thread::spawn(move || {
                let mut tick: u16 = 0;
                let mut pet = [0u8; 12];
                while flujo.read_exact(&mut pet).is_ok() {
                    tick = tick.wrapping_add(1);
                    let cantidad = u16::from_be_bytes([pet[10], pet[11]]);
                    let mut pdu = vec![pet[7], (cantidad * 2) as u8];
                    for i in 0..cantidad {
                        let v = 215 + (tick % 7) + i * 100; // décimas de grado, datos inventados
                        pdu.extend_from_slice(&v.to_be_bytes());
                    }
                    let largo = (pdu.len() + 1) as u16;
                    let mut r = vec![pet[0], pet[1], 0, 0];
                    r.extend_from_slice(&largo.to_be_bytes());
                    r.push(pet[6]);
                    r.extend_from_slice(&pdu);
                    if flujo.write_all(&r).is_err() {
                        return;
                    }
                }
            });
        }
    });
    Ok(direccion)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let direccion = equipo_simulado()?;
    println!("equipo simulado en {direccion}\n");

    let mut cliente = Cliente::conectar(direccion, 1, Duration::from_secs(1), Duration::from_secs(1))?;
    let valores = cliente.leer(Funcion::RegistrosRetencion, 0, 3)?;
    println!("lectura directa, 3 registros de retención: {valores:?}");

    let sondeador = Sondeador::iniciar(
        vec![Equipo {
            nombre: "cámara de frío 2".into(),
            direccion,
            unidad: 1,
            bloques: vec![Bloque {
                etiqueta: "temperatura (décimas de °C)".into(),
                funcion: Funcion::RegistrosEntrada,
                direccion: 0,
                cantidad: 1,
            }],
        }],
        Duration::from_millis(300),
        Duration::from_secs(1),
        Duration::from_secs(1),
    );

    println!("\nsondeo en segundo plano:");
    for _ in 0..5 {
        thread::sleep(Duration::from_millis(350));
        for l in sondeador.lecturas() {
            let valores: Vec<String> = l
                .bloques
                .iter()
                .map(|b| match &b.valores {
                    Ok(v) => format!("{} = {v:?}", b.etiqueta),
                    Err(e) => format!("{} = error: {e}", b.etiqueta),
                })
                .collect();
            println!("  {:<18} {:?}  {}", l.equipo, l.estado, valores.join(", "));
        }
    }
    sondeador.detener();
    Ok(())
}
