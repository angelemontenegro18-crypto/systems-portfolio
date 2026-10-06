//! Escritura y lectura little-endian: el orden de bytes exacto, la ida y vuelta, y que lo
//! que no cabe o falta es un error que no mueve el cursor.

use state_handoff::le::{Escritor, Lector};
use state_handoff::Error;

#[test]
fn el_orden_de_bytes_es_little_endian() {
    let mut salida = [0u8; 15];
    let mut e = Escritor::nuevo(&mut salida);
    e.u8(0xAB).unwrap();
    e.u16_le(0x1234).unwrap();
    e.u32_le(0x0102_0304).unwrap();
    e.u64_le(0x1122_3344_5566_7788).unwrap();
    assert_eq!(e.posicion(), 15);
    assert_eq!(
        salida,
        [
            0xAB, 0x34, 0x12, 0x04, 0x03, 0x02, 0x01, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
            0x11
        ]
    );
}

#[test]
fn ida_y_vuelta() {
    let mut salida = [0u8; 19];
    let mut e = Escritor::nuevo(&mut salida);
    e.u8(7).unwrap();
    e.u16_le(u16::MAX - 1).unwrap();
    e.u32_le(0xDEAD_BEEF).unwrap();
    e.u64_le(u64::MAX / 3).unwrap();
    e.bytes(b"TRAS").unwrap();
    let mut l = Lector::nuevo(&salida);
    assert_eq!(l.u8(), Ok(7));
    assert_eq!(l.u16_le(), Ok(u16::MAX - 1));
    assert_eq!(l.u32_le(), Ok(0xDEAD_BEEF));
    assert_eq!(l.u64_le(), Ok(u64::MAX / 3));
    assert_eq!(l.bytes::<4>(), Ok(*b"TRAS"));
    assert_eq!(l.posicion(), 19);
    assert!(l.resto().is_empty());
}

#[test]
fn lo_que_no_cabe_es_un_error_y_no_escribe_nada() {
    let mut salida = [9u8; 5];
    let mut e = Escritor::nuevo(&mut salida);
    e.u32_le(1).unwrap();
    assert_eq!(e.u16_le(2), Err(Error::SalidaCorta));
    assert_eq!(e.posicion(), 4);
    e.u8(3).unwrap();
    assert_eq!(e.u8(4), Err(Error::SalidaCorta));
    assert_eq!(salida, [1, 0, 0, 0, 3]);
}

#[test]
fn lo_que_falta_es_un_error_y_no_mueve_el_cursor() {
    let entrada = [1u8, 2, 3];
    let mut l = Lector::nuevo(&entrada);
    assert_eq!(l.u32_le(), Err(Error::Truncado));
    assert_eq!(l.posicion(), 0);
    assert_eq!(l.u16_le(), Ok(0x0201));
    assert_eq!(l.u16_le(), Err(Error::Truncado));
    assert_eq!(l.resto(), &[3]);
    assert_eq!(l.u8(), Ok(3));
    assert_eq!(l.u8(), Err(Error::Truncado));
}
