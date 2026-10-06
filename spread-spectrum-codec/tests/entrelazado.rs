//! El entrelazador: ida y vuelta, la propiedad de las ráfagas (exhaustiva, y justa), y que
//! junto con Hamming (8,4) corrige cualquier ráfaga de hasta `D` bits.

use spread_spectrum_codec::entrelazado::{desentrelazar, desentrelazar_blandos, entrelazar};
use spread_spectrum_codec::hamming::{codificar, decodificar, Decodificacion};
use spread_spectrum_codec::Error;

fn palabras_de_prueba(d: usize) -> Vec<u8> {
    (0..d)
        .map(|r| (r as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

/// De qué palabra es cada bit de la trama, según el entrelazador mismo: se enciende una
/// palabra entera por vez y se mira dónde caen sus bits.
fn duenos(d: usize) -> Vec<usize> {
    let mut duenos = vec![usize::MAX; 8 * d];
    for r in 0..d {
        let mut palabras = vec![0u8; d];
        palabras[r] = 0xFF;
        let mut bits = vec![0u8; 8 * d];
        entrelazar(&palabras, &mut bits).unwrap();
        for (i, &b) in bits.iter().enumerate() {
            if b == 1 {
                assert_eq!(duenos[i], usize::MAX, "dos palabras en el bit {i}");
                duenos[i] = r;
            }
        }
    }
    assert!(duenos.iter().all(|&r| r < d), "bits sin dueño");
    duenos
}

/// Cuántas veces toca a cada palabra una ráfaga de `largo` bits desde `inicio`.
fn toques(duenos: &[usize], d: usize, inicio: usize, largo: usize) -> Vec<usize> {
    let mut toques = vec![0; d];
    for &r in &duenos[inicio..inicio + largo] {
        toques[r] += 1;
    }
    toques
}

#[test]
fn ida_y_vuelta_para_cada_profundidad() {
    for d in 0..=20 {
        let palabras = palabras_de_prueba(d);
        let mut bits = vec![9u8; 8 * d];
        assert_eq!(entrelazar(&palabras, &mut bits), Ok(8 * d));
        assert!(bits.iter().all(|&b| b <= 1));
        let mut vuelta = vec![0u8; d];
        assert_eq!(desentrelazar(&bits, &mut vuelta), Ok(d));
        assert_eq!(vuelta, palabras);
    }
}

#[test]
fn el_bit_j_de_la_palabra_r_va_a_la_posicion_j_por_d_mas_r() {
    let d = 5;
    for r in 0..d {
        for j in 0..8 {
            let mut palabras = vec![0u8; d];
            palabras[r] = 1 << j;
            let mut bits = vec![0u8; 8 * d];
            entrelazar(&palabras, &mut bits).unwrap();
            let unos: Vec<usize> = (0..bits.len()).filter(|&i| bits[i] == 1).collect();
            assert_eq!(unos, [j * d + r]);
        }
    }
}

#[test]
fn una_rafaga_de_hasta_d_bits_toca_cada_palabra_a_lo_sumo_una_vez() {
    for d in 1..=16 {
        let duenos = duenos(d);
        for largo in 1..=d {
            for inicio in 0..=8 * d - largo {
                let t = toques(&duenos, d, inicio, largo);
                assert!(
                    t.iter().all(|&x| x <= 1),
                    "D = {d}, ráfaga de {largo} desde {inicio}"
                );
            }
        }
    }
}

#[test]
fn una_rafaga_de_d_mas_uno_bits_ya_toca_una_palabra_dos_veces() {
    // La cota es justa: con un bit más ya no vale.
    for d in 1..=16 {
        let duenos = duenos(d);
        for inicio in 0..=8 * d - (d + 1) {
            let t = toques(&duenos, d, inicio, d + 1);
            assert!(t.contains(&2), "D = {d}, ráfaga desde {inicio}");
        }
    }
}

#[test]
fn con_hamming_cualquier_rafaga_de_hasta_d_bits_se_corrige() {
    for d in 1..=12 {
        let datos: Vec<u8> = (0..d).map(|r| (r * 7 % 16) as u8).collect();
        let palabras: Vec<u8> = datos.iter().map(|&x| codificar(x).unwrap()).collect();
        let mut limpios = vec![0u8; 8 * d];
        entrelazar(&palabras, &mut limpios).unwrap();
        for largo in 1..=d {
            for inicio in 0..=8 * d - largo {
                let mut bits = limpios.clone();
                for b in &mut bits[inicio..inicio + largo] {
                    *b ^= 1;
                }
                let mut recibidas = vec![0u8; d];
                desentrelazar(&bits, &mut recibidas).unwrap();
                for (&w, &x) in recibidas.iter().zip(&datos) {
                    let bien = matches!(
                        decodificar(w),
                        Decodificacion::Intacta(y) | Decodificacion::Corregida(y) if y == x
                    );
                    assert!(bien, "D = {d}, ráfaga de {largo} desde {inicio}");
                }
            }
        }
    }
}

#[test]
fn sin_entrelazar_dos_bits_seguidos_rompen_una_palabra() {
    // El control: con las palabras en orden, una ráfaga de dos bits que cae dentro de una
    // palabra ya no se corrige. Es lo que el entrelazador evita.
    let w = codificar(9).unwrap();
    assert_eq!(decodificar(w ^ 0b0000_1100), Decodificacion::DobleError);
}

#[test]
fn los_blandos_se_desentrelazan_igual_que_los_bits() {
    let d = 6;
    let palabras = palabras_de_prueba(d);
    let mut bits = vec![0u8; 8 * d];
    entrelazar(&palabras, &mut bits).unwrap();
    // Cada bit como un valor blando distinto, con el signo del bit.
    let blandos: Vec<i64> = bits
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if b == 0 {
                100 + i as i64
            } else {
                -100 - i as i64
            }
        })
        .collect();
    let mut por_palabra = vec![[0i64; 8]; d];
    assert_eq!(desentrelazar_blandos(&blandos, &mut por_palabra), Ok(d));
    for (r, (valores, &w)) in por_palabra.iter().zip(&palabras).enumerate() {
        for (j, &v) in valores.iter().enumerate() {
            assert_eq!(v < 0, (w >> j) & 1 == 1);
            assert_eq!(v.abs(), 100 + (j * d + r) as i64);
        }
    }
}

#[test]
fn las_entradas_invalidas_se_rechazan() {
    let mut bits = [0u8; 15];
    assert_eq!(entrelazar(&[1, 2], &mut bits), Err(Error::SalidaCorta));
    let mut palabras = [0u8; 2];
    assert_eq!(
        desentrelazar(&bits, &mut palabras),
        Err(Error::LargoNoMultiplo)
    );
    let mut con_un_dos = [0u8; 16];
    con_un_dos[3] = 2;
    assert_eq!(
        desentrelazar(&con_un_dos, &mut palabras),
        Err(Error::BitInvalido)
    );
    assert_eq!(
        desentrelazar(&[0u8; 24], &mut palabras),
        Err(Error::SalidaCorta)
    );
    let mut blandos = [[0i64; 8]; 1];
    assert_eq!(
        desentrelazar_blandos(&[0i64; 12], &mut blandos),
        Err(Error::LargoNoMultiplo)
    );
    assert_eq!(
        desentrelazar_blandos(&[0i64; 16], &mut blandos),
        Err(Error::SalidaCorta)
    );
    // Un bloque vacío no es un error.
    assert_eq!(entrelazar(&[], &mut []), Ok(0));
    assert_eq!(desentrelazar(&[], &mut []), Ok(0));
}
