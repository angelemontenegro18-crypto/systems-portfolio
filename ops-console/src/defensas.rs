//! Defensas contra el navegador del usuario.
//!
//! Que el servidor escuche solo en `127.0.0.1` no basta: el navegador del usuario sí puede
//! llegar a él, y cualquier página que tenga abierta puede intentar usarlo.
//!
//! - **Rebinding de DNS.** Una página de `atacante.example` cambia su DNS para que apunte a
//!   `127.0.0.1` y queda «del mismo origen» que la consola. Lo que la delata es la cabecera
//!   `Host`, que sigue diciendo `atacante.example`: [`verificar_host`] solo acepta los
//!   nombres del propio servidor y responde 421 a cualquier otro.
//! - **CSRF.** Un formulario o un `fetch` simple de otra página no puede mandar
//!   `Content-Type: application/json` ni una cabecera propia sin un permiso previo (CORS) que
//!   este servidor nunca da. [`verificar_post`] exige las dos cosas en todo `POST` (415 y
//!   403), y si llega `Origin` tiene que coincidir con `Host` (403).
//! - **Sin CORS.** Ninguna respuesta lleva cabeceras `Access-Control-*`.

use std::net::SocketAddr;

use crate::http::{Cabeza, Rechazo};

/// La cabecera propia que exige todo `POST`, y su único valor válido.
pub const CABECERA_PROPIA: (&str, &str) = ("x-consola", "1");

/// Los valores de `Host` que el servidor acepta cuando escucha en `escucha`: su propia
/// dirección, `localhost`, `127.0.0.1` y `[::1]`, con su puerto. En el puerto 80 el navegador
/// no lo escribe, así que entonces también valen sin él.
pub fn hosts_validos(escucha: SocketAddr) -> Vec<String> {
    let puerto = escucha.port();
    let propia = match escucha {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
    };
    let mut nombres = vec![
        "127.0.0.1".to_string(),
        "localhost".to_string(),
        "[::1]".to_string(),
    ];
    if !nombres.contains(&propia) {
        nombres.push(propia);
    }
    let mut validos: Vec<String> = nombres.iter().map(|n| format!("{n}:{puerto}")).collect();
    if puerto == 80 {
        validos.extend(nombres);
    }
    validos
}

/// 421 si `Host` no es uno de los del propio servidor.
pub fn verificar_host(cabeza: &Cabeza, escucha: SocketAddr) -> Result<(), Rechazo> {
    let host = cabeza.cabecera("host").unwrap_or_default();
    let valido = hosts_validos(escucha)
        .iter()
        .any(|h| h.eq_ignore_ascii_case(host));
    if valido {
        Ok(())
    } else {
        Err(Rechazo::nuevo(
            421,
            "host_ajeno",
            "la cabecera Host no es la de este servidor (¿rebinding de DNS?)",
        ))
    }
}

/// Las condiciones de todo `POST`: `Content-Type: application/json` (415), la cabecera
/// propia (403), `Origin` igual al servidor si viene (403), y `Sec-Fetch-Site` del mismo
/// origen si viene (403).
pub fn verificar_post(cabeza: &Cabeza) -> Result<(), Rechazo> {
    let tipo = cabeza.cabecera("content-type").unwrap_or_default();
    let medio = tipo.split(';').next().unwrap_or_default().trim();
    if !medio.eq_ignore_ascii_case("application/json") {
        return Err(Rechazo::nuevo(
            415,
            "tipo_no_admitido",
            "los POST exigen Content-Type: application/json",
        ));
    }
    let (nombre, valor) = CABECERA_PROPIA;
    if cabeza.cabecera(nombre) != Some(valor) {
        return Err(Rechazo::nuevo(
            403,
            "falta_cabecera_propia",
            "los POST exigen la cabecera X-Consola: 1",
        ));
    }
    if let Some(origen) = cabeza.cabecera("origin") {
        let host = cabeza.cabecera("host").unwrap_or_default();
        let esperado = format!("http://{host}");
        if !origen.eq_ignore_ascii_case(&esperado) {
            return Err(Rechazo::nuevo(
                403,
                "origen_ajeno",
                "la cabecera Origin no es la de este servidor",
            ));
        }
    }
    if let Some(sitio) = cabeza.cabecera("sec-fetch-site") {
        if !matches!(sitio, "same-origin" | "none") {
            return Err(Rechazo::nuevo(
                403,
                "origen_ajeno",
                "la petición viene de otro sitio (Sec-Fetch-Site)",
            ));
        }
    }
    Ok(())
}
