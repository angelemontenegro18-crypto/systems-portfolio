//! Comportamiento contra un equipo simulado en el mismo proceso.
//!
//! El simulador decodifica las peticiones byte a byte por su cuenta: no usa el
//! códec del crate, así que un error simétrico (codificar y decodificar mal de
//! la misma forma) no pasaría inadvertido.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use modbus_ro::{
    Bloque, Cliente, Equipo, ErrorModbus, ErrorTrama, EstadoEquipo, Funcion, Sondeador,
};

#[derive(Clone, Copy)]
enum Modo {
    /// Responde bien; las direcciones desde 1000 devuelven la excepción 2.
    Normal,
    /// Lee la petición y no contesta nunca.
    Mudo,
    /// Contesta con el identificador de transacción cambiado.
    OtraTransaccion,
}

/// Valor determinista de cada registro, para poder comprobar lo leído.
fn valor(funcion: u8, direccion: u16) -> u16 {
    if funcion == 3 {
        direccion.wrapping_mul(10)
    } else {
        direccion.wrapping_add(1000)
    }
}

fn simulador(modo: Modo) -> SocketAddr {
    let oyente = TcpListener::bind("127.0.0.1:0").expect("bind");
    let direccion = oyente.local_addr().expect("dirección local");
    thread::spawn(move || {
        for conexion in oyente.incoming().flatten() {
            thread::spawn(move || atender(conexion, modo));
        }
    });
    direccion
}

fn atender(mut flujo: TcpStream, modo: Modo) {
    loop {
        let mut pet = [0u8; 12];
        if flujo.read_exact(&mut pet).is_err() {
            return;
        }
        let tx = u16::from_be_bytes([pet[0], pet[1]]);
        let unidad = pet[6];
        let funcion = pet[7];
        let dir = u16::from_be_bytes([pet[8], pet[9]]);
        let cantidad = u16::from_be_bytes([pet[10], pet[11]]);

        if let Modo::Mudo = modo {
            thread::sleep(Duration::from_secs(60));
            return;
        }

        let pdu: Vec<u8> = if dir >= 1000 {
            vec![funcion | 0x80, 2]
        } else {
            let mut p = vec![funcion, u8::try_from(cantidad * 2).expect("cantidad chica")];
            for i in 0..cantidad {
                p.extend_from_slice(&valor(funcion, dir + i).to_be_bytes());
            }
            p
        };
        let tx_respuesta = match modo {
            Modo::OtraTransaccion => tx.wrapping_add(1),
            _ => tx,
        };
        let largo = u16::try_from(pdu.len() + 1).expect("pdu chico");
        let mut respuesta = Vec::new();
        respuesta.extend_from_slice(&tx_respuesta.to_be_bytes());
        respuesta.extend_from_slice(&[0, 0]);
        respuesta.extend_from_slice(&largo.to_be_bytes());
        respuesta.push(unidad);
        respuesta.extend_from_slice(&pdu);
        if flujo.write_all(&respuesta).is_err() {
            return;
        }
    }
}

fn conectar(direccion: SocketAddr, espera: Duration) -> Cliente<TcpStream> {
    Cliente::conectar(direccion, 1, Duration::from_secs(2), espera).expect("conectar al simulador")
}

#[test]
fn lee_registros_de_retencion_y_de_entrada() {
    let mut c = conectar(simulador(Modo::Normal), Duration::from_secs(2));
    assert_eq!(
        c.leer_retencion(10, 3).expect("retención"),
        vec![100, 110, 120]
    );
    assert_eq!(c.leer_entrada(5, 2).expect("entrada"), vec![1005, 1006]);
    // Varias lecturas seguidas sobre la misma conexión: las transacciones avanzan.
    for _ in 0..20 {
        assert_eq!(
            c.leer(Funcion::RegistrosRetencion, 0, 1).expect("lectura"),
            vec![0]
        );
    }
}

#[test]
fn una_excepcion_del_equipo_llega_como_error_con_nombre() {
    let mut c = conectar(simulador(Modo::Normal), Duration::from_secs(2));
    let e = c
        .leer_retencion(1000, 1)
        .expect_err("dirección fuera del mapa");
    assert!(
        matches!(e, ErrorModbus::Trama(ErrorTrama::Excepcion { codigo: 2 })),
        "{e:?}"
    );
    assert!(e.to_string().contains("dirección fuera del mapa"), "{e}");
    // La excepción no desincroniza el flujo: la siguiente lectura funciona.
    assert_eq!(c.leer_retencion(1, 1).expect("lectura posterior"), vec![10]);
}

#[test]
fn un_equipo_mudo_produce_timeout_y_no_cuelga() {
    let mut c = conectar(simulador(Modo::Mudo), Duration::from_millis(200));
    let inicio = Instant::now();
    let e = c.leer_retencion(0, 1).expect_err("el equipo no contesta");
    assert!(e.es_timeout(), "{e:?}");
    assert!(
        inicio.elapsed() < Duration::from_secs(2),
        "tardó {:?}",
        inicio.elapsed()
    );
}

#[test]
fn una_respuesta_de_otra_transaccion_se_rechaza() {
    let mut c = conectar(simulador(Modo::OtraTransaccion), Duration::from_secs(2));
    let e = c.leer_retencion(0, 1).expect_err("transacción ajena");
    assert!(
        matches!(e, ErrorModbus::Trama(ErrorTrama::OtraTransaccion { .. })),
        "{e:?}"
    );
}

#[test]
fn una_peticion_invalida_no_llega_a_la_red() {
    // Se rechaza antes de escribir en el socket: el error es de la petición, no del equipo.
    let mut c = conectar(simulador(Modo::Normal), Duration::from_secs(2));
    let e = c
        .leer_retencion(0, 200)
        .expect_err("cantidad fuera de rango");
    assert!(matches!(e, ErrorModbus::PeticionInvalida(_)), "{e:?}");
}

fn equipo(nombre: &str, direccion: SocketAddr, bloques: &[(u16, u16)]) -> Equipo {
    Equipo {
        nombre: nombre.to_string(),
        direccion,
        unidad: 1,
        bloques: bloques
            .iter()
            .map(|&(dir, cant)| Bloque {
                etiqueta: format!("bloque {dir}"),
                funcion: Funcion::RegistrosRetencion,
                direccion: dir,
                cantidad: cant,
            })
            .collect(),
    }
}

fn puerto_cerrado() -> SocketAddr {
    let oyente = TcpListener::bind("127.0.0.1:0").expect("bind");
    let direccion = oyente.local_addr().expect("dirección local");
    drop(oyente);
    direccion
}

#[test]
fn el_sondeador_distingue_en_linea_degradado_y_fuera_de_linea() {
    let sano = simulador(Modo::Normal);
    let sondeador = Sondeador::iniciar(
        vec![
            equipo("sano", sano, &[(0, 2), (10, 1)]),
            equipo("a medias", sano, &[(0, 1), (1000, 1)]),
            equipo("apagado", puerto_cerrado(), &[(0, 1)]),
        ],
        Duration::from_millis(100),
        Duration::from_millis(500),
        Duration::from_millis(500),
    );

    let limite = Instant::now() + Duration::from_secs(5);
    let lecturas = loop {
        let l = sondeador.lecturas();
        if l.iter().all(|x| x.tomada.is_some()) {
            break l;
        }
        assert!(Instant::now() < limite, "el primer ciclo no terminó: {l:?}");
        thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(lecturas[0].estado, EstadoEquipo::EnLinea);
    assert_eq!(lecturas[0].bloques[0].valores, Ok(vec![0, 10]));
    assert_eq!(lecturas[1].estado, EstadoEquipo::Degradado);
    assert_eq!(lecturas[2].estado, EstadoEquipo::FueraDeLinea);
    assert!(lecturas[2].error.is_some());
    sondeador.detener();
}

#[test]
fn el_sondeador_sirve_el_cache_sin_esperar_a_la_red() {
    let sondeador = Sondeador::iniciar(
        vec![equipo("mudo", simulador(Modo::Mudo), &[(0, 1)])],
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_millis(1500),
    );
    // Se da tiempo a que el hilo quede bloqueado esperando la respuesta.
    thread::sleep(Duration::from_millis(150));

    for _ in 0..50 {
        let inicio = Instant::now();
        let l = sondeador.lecturas();
        assert!(
            inicio.elapsed() < Duration::from_millis(50),
            "lecturas() esperó {:?}",
            inicio.elapsed()
        );
        assert_eq!(
            l[0].estado,
            EstadoEquipo::Sondeando,
            "el primer ciclo sigue en curso"
        );
    }
}
