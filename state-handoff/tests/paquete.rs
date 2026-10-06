//! El paquete: su forma byte a byte, la ida y vuelta, la detección exhaustiva de todo error de
//! uno y dos bits, y el rechazo de lo truncado, lo extendido, la firma, la versión y el
//! destino ajenos.

use state_handoff::cabecera::{Cabecera, FIRMA, LARGO_CABECERA, VERSION};
use state_handoff::crc32::crc32;
use state_handoff::paquete::{sellar, Paquete};
use state_handoff::Error;

const ORIGEN: u16 = 1;
const PROPIO: u16 = 2;

/// Un paquete chico: 6 bytes de contenido, 36 en total.
fn paquete_chico() -> Vec<u8> {
    let mut bytes = vec![0u8; 64];
    let n = sellar(
        &[11, 22, 33, 44, 55, 66],
        ORIGEN,
        PROPIO,
        0x0102_0304_0506_0708,
        &mut bytes,
    )
    .unwrap();
    bytes.truncate(n);
    bytes
}

fn verificar(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    Paquete::leer(bytes)?
        .verificar(PROPIO)
        .map(|p| p.contenido().to_vec())
}

/// Reescribe el CRC de la cabecera: el que haría alguien que cambia un campo «bien». La
/// posición (26..30) está escrita aquí a mano, como referencia del formato documentado.
fn recalcular_crc_de_cabecera(bytes: &mut [u8]) {
    let crc = crc32(&bytes[..26]);
    bytes[26..30].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn la_cabecera_tiene_la_forma_documentada_byte_a_byte() {
    assert_eq!(LARGO_CABECERA, 30);
    let contenido = [11u8, 22, 33, 44, 55, 66];
    let b = paquete_chico();
    assert_eq!(b.len(), 36);
    assert_eq!(&b[0..4], b"TRAS");
    assert_eq!(&b[4..6], &[1, 0]);
    assert_eq!(&b[6..8], &ORIGEN.to_le_bytes());
    assert_eq!(&b[8..10], &PROPIO.to_le_bytes());
    assert_eq!(&b[10..18], &[8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(&b[18..22], &6u32.to_le_bytes());
    assert_eq!(&b[22..26], &crc32(&contenido).to_le_bytes());
    assert_eq!(&b[26..30], &crc32(&b[..26]).to_le_bytes());
    assert_eq!(&b[30..], &contenido);
}

#[test]
fn ida_y_vuelta() {
    let b = paquete_chico();
    let verificado = Paquete::leer(&b).unwrap().verificar(PROPIO).unwrap();
    assert_eq!(verificado.contenido(), &[11, 22, 33, 44, 55, 66]);
    assert_eq!(verificado.epoca(), 0x0102_0304_0506_0708);
    assert_eq!(verificado.origen(), ORIGEN);
    assert_eq!(
        *verificado.cabecera(),
        Cabecera {
            firma: FIRMA,
            version: VERSION,
            origen: ORIGEN,
            destino: PROPIO,
            epoca: 0x0102_0304_0506_0708,
            largo: 6,
            crc_contenido: crc32(&[11, 22, 33, 44, 55, 66]),
            crc_cabecera: crc32(&b[..26]),
        }
    );
}

#[test]
fn un_contenido_vacio_tambien_viaja() {
    let mut b = [0u8; 30];
    assert_eq!(sellar(&[], ORIGEN, PROPIO, 1, &mut b), Ok(30));
    assert_eq!(verificar(&b), Ok(vec![]));
}

#[test]
fn todo_error_de_un_bit_se_detecta() {
    let b = paquete_chico();
    for i in 0..b.len() * 8 {
        let mut x = b.clone();
        x[i / 8] ^= 1 << (i % 8);
        assert!(verificar(&x).is_err(), "bit {i}");
    }
}

#[test]
fn todo_error_de_dos_bits_se_detecta() {
    let b = paquete_chico();
    let bits = b.len() * 8;
    let mut casos = 0;
    for i in 0..bits {
        for j in i + 1..bits {
            let mut x = b.clone();
            x[i / 8] ^= 1 << (i % 8);
            x[j / 8] ^= 1 << (j % 8);
            assert!(verificar(&x).is_err(), "bits {i} y {j}");
            casos += 1;
        }
    }
    assert_eq!(casos, 288 * 287 / 2);
}

#[test]
fn con_la_cabecera_intacta_el_crc_del_contenido_es_el_que_atrapa() {
    // Cada error de un bit en el contenido llega hasta la última defensa: el CRC del
    // contenido, y no otra.
    let b = paquete_chico();
    for i in LARGO_CABECERA * 8..b.len() * 8 {
        let mut x = b.clone();
        x[i / 8] ^= 1 << (i % 8);
        assert_eq!(verificar(&x), Err(Error::ContenidoCorrupto), "bit {i}");
    }
}

#[test]
fn un_paquete_truncado_se_rechaza_en_cualquier_largo() {
    let b = paquete_chico();
    for largo in 0..b.len() {
        assert_eq!(
            verificar(&b[..largo]),
            Err(Error::Truncado),
            "largo {largo}"
        );
    }
}

#[test]
fn un_paquete_extendido_se_rechaza() {
    for extra in 1..=5 {
        let mut b = paquete_chico();
        b.extend(std::iter::repeat_n(0, extra));
        assert_eq!(verificar(&b), Err(Error::Sobrante), "{extra} de más");
    }
}

#[test]
fn una_firma_ajena_se_rechaza_aunque_su_crc_cierre() {
    let mut b = paquete_chico();
    b[0..4].copy_from_slice(b"OTRO");
    recalcular_crc_de_cabecera(&mut b);
    assert_eq!(verificar(&b), Err(Error::FirmaAjena));
}

#[test]
fn una_version_desconocida_se_rechaza_aunque_su_crc_cierre() {
    let mut b = paquete_chico();
    b[4..6].copy_from_slice(&2u16.to_le_bytes());
    recalcular_crc_de_cabecera(&mut b);
    assert_eq!(verificar(&b), Err(Error::VersionDesconocida(2)));
}

#[test]
fn un_paquete_para_otro_nodo_se_rechaza() {
    let mut b = vec![0u8; 64];
    let n = sellar(&[1, 2, 3], ORIGEN, 3, 5, &mut b).unwrap();
    assert_eq!(verificar(&b[..n]), Err(Error::DestinoAjeno(3)));
    // Para el nodo 3, el mismo paquete es válido.
    assert!(Paquete::leer(&b[..n]).unwrap().verificar(3).is_ok());
}

#[test]
fn un_largo_mentiroso_con_crc_que_cierra_se_rechaza() {
    // La cabecera anuncia 7 bytes de contenido y trae 6: el CRC de la cabecera cierra (se
    // recalculó), pero el largo no.
    let mut b = paquete_chico();
    b[18..22].copy_from_slice(&7u32.to_le_bytes());
    recalcular_crc_de_cabecera(&mut b);
    assert_eq!(verificar(&b), Err(Error::Truncado));
    b[18..22].copy_from_slice(&5u32.to_le_bytes());
    recalcular_crc_de_cabecera(&mut b);
    assert_eq!(verificar(&b), Err(Error::Sobrante));
}

#[test]
fn sellar_en_un_bufer_chico_es_un_error() {
    let mut b = [0u8; 35];
    assert_eq!(
        sellar(&[1, 2, 3, 4, 5, 6], ORIGEN, PROPIO, 1, &mut b),
        Err(Error::SalidaCorta)
    );
    assert_eq!(b, [0; 35]);
}
