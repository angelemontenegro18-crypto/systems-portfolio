//! El formato de la cabecera, con su largo atado a sus campos en compilación.
//!
//! La cabecera se declara **una sola vez**, como una lista de campos con su tipo, con
//! [`formato_de_cabecera!`](crate::formato_de_cabecera). De esa lista salen el `struct`, la
//! escritura y la lectura en little-endian (en el orden de la lista), y una aserción que se
//! evalúa al compilar: el largo declarado tiene que ser la suma de los anchos de los campos.
//! Agregar un campo sin actualizar el largo no compila.
//!
//! | bytes   | campo           | tipo      |
//! |---------|-----------------|-----------|
//! | 0..4    | `firma`         | `[u8; 4]` |
//! | 4..6    | `version`       | `u16`     |
//! | 6..8    | `origen`        | `u16`     |
//! | 8..10   | `destino`       | `u16`     |
//! | 10..18  | `epoca`         | `u64`     |
//! | 18..22  | `largo`         | `u32`     |
//! | 22..26  | `crc_contenido` | `u32`     |
//! | 26..30  | `crc_cabecera`  | `u32`     |
//!
//! El CRC de la cabecera es el último campo y cubre todos los anteriores.

/// Declara el formato de una cabecera: el `struct`, su largo, su escritura y su lectura en
/// little-endian, y la aserción en compilación de que el largo es la suma de los anchos.
///
/// Con un campo de más y el largo sin actualizar, la aserción falla al compilar:
///
/// ```compile_fail,E0080
/// state_handoff::formato_de_cabecera! {
///     /// La cabecera, con un campo nuevo de prioridad.
///     pub struct Cabecera, largo LARGO = 30 {
///         /// Firma.
///         firma: [u8; 4],
///         /// Versión.
///         version: u16,
///         /// Origen.
///         origen: u16,
///         /// Destino.
///         destino: u16,
///         /// Época.
///         epoca: u64,
///         /// Largo del contenido.
///         largo: u32,
///         /// CRC del contenido.
///         crc_contenido: u32,
///         /// CRC de la cabecera.
///         crc_cabecera: u32,
///         /// El campo nuevo. El largo sigue en 30: error[E0080].
///         prioridad: u8,
///     }
/// }
/// ```
///
/// Con el largo actualizado a 31, compila:
///
/// ```
/// state_handoff::formato_de_cabecera! {
///     /// La cabecera, con un campo nuevo de prioridad.
///     pub struct Cabecera, largo LARGO = 31 {
///         /// Firma.
///         firma: [u8; 4],
///         /// Versión.
///         version: u16,
///         /// Origen.
///         origen: u16,
///         /// Destino.
///         destino: u16,
///         /// Época.
///         epoca: u64,
///         /// Largo del contenido.
///         largo: u32,
///         /// CRC del contenido.
///         crc_contenido: u32,
///         /// CRC de la cabecera.
///         crc_cabecera: u32,
///         /// El campo nuevo.
///         prioridad: u8,
///     }
/// }
/// assert_eq!(LARGO, 31);
/// ```
#[macro_export]
macro_rules! formato_de_cabecera {
    (
        $(#[$atributo:meta])*
        pub struct $nombre:ident, largo $largo:ident = $valor:literal {
            $( $(#[$documento:meta])* $campo:ident : $tipo:ty ),+ $(,)?
        }
    ) => {
        $(#[$atributo])*
        pub struct $nombre {
            $( $(#[$documento])* pub $campo: $tipo, )+
        }

        #[doc = "El largo de la cabecera en bytes: la suma de los anchos de sus campos."]
        pub const $largo: usize = $valor;

        const _: () = ::core::assert!(
            $valor == 0 $( + ::core::mem::size_of::<$tipo>() )+,
            "el largo declarado de la cabecera no es la suma de los anchos de sus campos"
        );

        impl $nombre {
            #[doc = "Escribe los campos en el orden en que se declararon, en little-endian."]
            pub fn escribir_campos(
                &self,
                escritor: &mut $crate::le::Escritor<'_>,
            ) -> ::core::result::Result<(), $crate::Error> {
                $( $crate::le::CampoLe::escribir(&self.$campo, escritor)?; )+
                ::core::result::Result::Ok(())
            }

            #[doc = "Lee los campos en el orden en que se declararon."]
            pub fn leer_campos(
                lector: &mut $crate::le::Lector<'_>,
            ) -> ::core::result::Result<Self, $crate::Error> {
                ::core::result::Result::Ok(Self {
                    $( $campo: $crate::le::CampoLe::leer(lector)?, )+
                })
            }
        }
    };
}

crate::formato_de_cabecera! {
    /// La cabecera de un paquete de traspaso.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Cabecera, largo LARGO_CABECERA = 30 {
        /// La firma del protocolo: [`FIRMA`].
        firma: [u8; 4],
        /// La versión del formato: [`VERSION`].
        version: u16,
        /// El nodo que sella.
        origen: u16,
        /// El nodo al que va.
        destino: u16,
        /// La época del estado: el respaldo solo acepta épocas mayores que la última que
        /// confirmó.
        epoca: u64,
        /// El largo del contenido, en bytes.
        largo: u32,
        /// El CRC-32 del contenido.
        crc_contenido: u32,
        /// El CRC-32 de los bytes anteriores de la cabecera.
        crc_cabecera: u32,
    }
}

/// La firma del protocolo: los cuatro primeros bytes de todo paquete.
pub const FIRMA: [u8; 4] = *b"TRAS";

/// La versión del formato que este código escribe y entiende.
pub const VERSION: u16 = 1;

/// Dónde empieza el CRC de la cabecera: lo que va antes es lo que cubre.
pub const DESPLAZAMIENTO_CRC_CABECERA: usize = LARGO_CABECERA - core::mem::size_of::<u32>();
