//! CRC-32 de IEEE 802.3 —el de zlib, PNG y Ethernet—: polinomio reflejado `0xEDB88320`,
//! valor inicial y XOR final `0xFFFFFFFF`.
//!
//! La tabla de 256 entradas se calcula en compilación, recorriéndola sin indexar. El CRC
//! detecta errores accidentales —en particular, todo error de uno o dos bits en un paquete
//! chico, lo que las pruebas comprueban de forma exhaustiva—, pero **no autentica**:
//! cualquiera que cambie el contenido puede recalcularlo.
//!
//! ```
//! use state_handoff::crc32::crc32;
//!
//! // El valor de verificación de la norma.
//! assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
//! ```

/// El polinomio `x³² + x²⁶ + x²³ + … + 1`, reflejado.
const POLINOMIO: u32 = 0xEDB8_8320;

/// El CRC de cada byte posible, calculado en compilación.
const TABLA: [u32; 256] = {
    let mut tabla = [0u32; 256];
    let mut resto: &mut [u32] = &mut tabla;
    let mut byte = 0u32;
    while let Some((celda, cola)) = resto.split_first_mut() {
        let mut c = byte;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                POLINOMIO ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        *celda = c;
        resto = cola;
        byte += 1;
    }
    tabla
};

/// El CRC-32 de `datos`.
pub fn crc32(datos: &[u8]) -> u32 {
    let mut estado = 0xFFFF_FFFFu32;
    for &byte in datos {
        let indice = usize::from(byte ^ (estado as u8));
        // El índice es un byte: la tabla tiene una entrada para cada uno.
        let entrada = TABLA.get(indice).copied().unwrap_or(0);
        estado = entrada ^ (estado >> 8);
    }
    estado ^ 0xFFFF_FFFF
}
