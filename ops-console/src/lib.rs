//! # ops-console
//!
//! Una consola de operaciones **local** para un servicio de miniaturas (inventado): un grupo
//! de procesos trabajadores que redimensionan imágenes, el estado de sus servicios, la cola
//! de su registro con lo sensible tapado, y qué configuración tiene definida. Un servidor HTTP
//! propio, escrito sobre `std`, la sirve en la propia máquina.
//!
//! Lo que el diseño garantiza, y cómo:
//!
//! - **Solo loopback.** [`Servidor::escuchar`](servidor::Servidor::escuchar) exige una
//!   [`DireccionLocal`](direccion::DireccionLocal), y esa dirección solo existe si es
//!   `127.0.0.0/8` o `::1`. Escuchar en `0.0.0.0` no compila.
//! - **HTTP estricto y acotado.** Límites de línea, cabeceras, cuerpo, tiempo y conexiones;
//!   una petición por conexión; nada de *chunked*; `Content-Length` repetido o
//!   `Transfer-Encoding` se rechazan ([`http`]).
//! - **El navegador no es cómplice.** `Host` del propio servidor (contra el rebinding de
//!   DNS), y en todo `POST` JSON, una cabecera propia y el mismo origen (contra CSRF); ninguna
//!   cabecera CORS ([`defensas`]). Cabeceras de seguridad en toda respuesta.
//! - **Grupo sin huérfanos.** Reciclar conserva la posición; el tick reemplaza lo que murió;
//!   soltar el grupo —también en un pánico— termina y espera a todos ([`grupo`]).
//! - **Registros sin secretos.** Lo que sale de la API es un
//!   [`Redactado`](registros::Redactado), y solo [`redactar`](registros::redactar) lo crea.
//! - **Configuración sin valores.** [`sistema`] solo sabe decir si una variable está
//!   definida.
//!
//! Sin dependencias: el HTTP y el JSON también están escritos sobre `std`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod api;
pub mod defensas;
pub mod direccion;
pub mod estaticos;
pub mod grupo;
pub mod http;
pub mod json;
pub mod registros;
pub mod reloj;
pub mod servidor;
pub mod sistema;
pub mod trabajador;
pub mod vigia;
