//! El CRC-32 contra los valores conocidos (calculados aparte con `zlib`) y contra una
//! implementación bit a bit, sin tabla, escrita aquí.

use state_handoff::crc32::crc32;

/// La definición bit a bit: la referencia independiente de la tabla.
fn crc32_bit_a_bit(datos: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in datos {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let sale = crc & 1;
            crc >>= 1;
            if sale == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

#[test]
fn el_valor_de_verificacion_de_la_norma() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32_bit_a_bit(b"123456789"), 0xCBF4_3926);
}

#[test]
fn otros_valores_conocidos() {
    // Calculados con `zlib.crc32` de Python.
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"a"), 0xE8B7_BE43);
    assert_eq!(
        crc32(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339
    );
}

#[test]
fn coincide_con_la_referencia_bit_a_bit() {
    // Cada byte suelto: recorre las 256 entradas de la tabla.
    for byte in 0..=255u8 {
        assert_eq!(crc32(&[byte]), crc32_bit_a_bit(&[byte]), "byte {byte}");
    }
    // Y datos al azar de todos los largos hasta 300, con semilla fija.
    let mut estado = 0x2026u64;
    let mut siguiente = || {
        estado = estado.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = estado;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u8
    };
    for largo in 0..300 {
        let datos: Vec<u8> = (0..largo).map(|_| siguiente()).collect();
        assert_eq!(crc32(&datos), crc32_bit_a_bit(&datos), "largo {largo}");
    }
}
