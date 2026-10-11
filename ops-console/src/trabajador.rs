//! El trabajador de demostración: calcula el tamaño de miniaturas.
//!
//! Lee una tarea por línea —`ANCHO ALTO LADO`— y responde una línea por tarea: `ok A H` con el
//! tamaño de la miniatura, o `error …`. Termina cuando se cierra su entrada, así que si quien
//! lo lanzó muere, él también termina. No abre puertos ni toca la red.

use std::io::{self, BufRead, Write};

/// El tamaño de la miniatura de una imagen de `ancho`×`alto` que cabe en un cuadrado de
/// `lado`: sin deformarla y sin agrandarla. `None` si alguna medida es cero.
///
/// ```
/// use ops_console::trabajador::miniatura;
///
/// assert_eq!(miniatura(4000, 3000, 400), Some((400, 300)));
/// assert_eq!(miniatura(1080, 1920, 256), Some((144, 256)));
/// assert_eq!(miniatura(200, 100, 400), Some((200, 100)));
/// ```
pub fn miniatura(ancho: u32, alto: u32, lado: u32) -> Option<(u32, u32)> {
    if ancho == 0 || alto == 0 || lado == 0 {
        return None;
    }
    if ancho <= lado && alto <= lado {
        return Some((ancho, alto));
    }
    let (a, h, l) = (u64::from(ancho), u64::from(alto), u64::from(lado));
    let escalar = |medida: u64, mayor: u64| {
        let redondeada = ((medida * l + mayor / 2) / mayor).max(1);
        u32::try_from(redondeada).unwrap_or(lado)
    };
    if a >= h {
        Some((lado, escalar(h, a)))
    } else {
        Some((escalar(a, h), lado))
    }
}

/// La respuesta a una línea de tarea.
pub fn responder(tarea: &str) -> String {
    let medidas: Vec<Option<u32>> = tarea.split_whitespace().map(|m| m.parse().ok()).collect();
    match medidas.as_slice() {
        [Some(ancho), Some(alto), Some(lado)] => match miniatura(*ancho, *alto, *lado) {
            Some((a, h)) => format!("ok {a} {h}"),
            None => "error medidas en cero".into(),
        },
        _ => "error se espera «ANCHO ALTO LADO»".into(),
    }
}

/// Atiende tareas hasta que se cierre la entrada. Devuelve cuántas atendió.
pub fn atender(entrada: impl BufRead, mut salida: impl Write) -> io::Result<u64> {
    let mut atendidas = 0;
    for linea in entrada.lines() {
        writeln!(salida, "{}", responder(&linea?))?;
        salida.flush()?;
        atendidas += 1;
    }
    Ok(atendidas)
}
