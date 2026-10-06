//! Hamming ampliado (8,4), de forma exhaustiva: los 16 datos con cero, uno, dos y tres errores
//! en todas las posiciones posibles, y la decodificación blanda.

use spread_spectrum_codec::hamming::{codificar, decodificar, decodificar_blando, Decodificacion};
use spread_spectrum_codec::Error;

/// Los 16 datos con su palabra.
fn palabras() -> Vec<(u8, u8)> {
    (0..16).map(|d| (d, codificar(d).unwrap())).collect()
}

/// Los valores blandos de una palabra recibida sin duda: `+magnitud` para el 0, `−magnitud`
/// para el 1.
fn blandos_de(palabra: u8, magnitud: i64) -> [i64; 8] {
    std::array::from_fn(|j| {
        if (palabra >> j) & 1 == 0 {
            magnitud
        } else {
            -magnitud
        }
    })
}

#[test]
fn solo_se_codifican_datos_de_cuatro_bits() {
    for d in 16..=255u8 {
        assert_eq!(codificar(d), Err(Error::FueraDeRango));
    }
}

#[test]
fn la_distancia_minima_es_cuatro_y_la_paridad_es_par() {
    let todas = palabras();
    let mut minima = u32::MAX;
    for (i, &(_, a)) in todas.iter().enumerate() {
        assert_eq!(a.count_ones() % 2, 0);
        for &(_, b) in &todas[i + 1..] {
            minima = minima.min((a ^ b).count_ones());
        }
    }
    assert_eq!(minima, 4);
}

#[test]
fn sin_errores_la_palabra_llega_intacta() {
    for (d, w) in palabras() {
        assert_eq!(decodificar(w), Decodificacion::Intacta(d));
    }
}

#[test]
fn cualquier_error_de_un_bit_se_corrige() {
    for (d, w) in palabras() {
        for i in 0..8 {
            assert_eq!(
                decodificar(w ^ (1 << i)),
                Decodificacion::Corregida(d),
                "dato {d}, bit {i}"
            );
        }
    }
}

#[test]
fn cualquier_error_de_dos_bits_se_detecta() {
    let mut casos = 0;
    for (d, w) in palabras() {
        for i in 0..8 {
            for j in i + 1..8 {
                assert_eq!(
                    decodificar(w ^ (1 << i) ^ (1 << j)),
                    Decodificacion::DobleError,
                    "dato {d}, bits {i} y {j}"
                );
                casos += 1;
            }
        }
    }
    assert_eq!(casos, 16 * 28);
}

#[test]
fn con_tres_errores_el_dato_sale_mal() {
    // El límite del código: tres errores se parecen a uno, y la palabra se «corrige» hacia
    // otra. Nunca sale como intacta ni con el dato bueno.
    for (d, w) in palabras() {
        for i in 0..8 {
            for j in i + 1..8 {
                for k in j + 1..8 {
                    match decodificar(w ^ (1 << i) ^ (1 << j) ^ (1 << k)) {
                        Decodificacion::Corregida(x) => assert_ne!(x, d),
                        otra => panic!("dato {d}, bits {i}, {j} y {k}: {otra:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn los_256_bytes_se_clasifican_sin_excepcion() {
    let (mut intactas, mut corregidas, mut dobles) = (0, 0, 0);
    for w in 0..=255u8 {
        match decodificar(w) {
            Decodificacion::Intacta(_) => intactas += 1,
            Decodificacion::Corregida(_) => corregidas += 1,
            Decodificacion::DobleError => dobles += 1,
        }
    }
    // 16 palabras válidas, 16 · 8 a distancia uno, y el resto a distancia dos.
    assert_eq!((intactas, corregidas, dobles), (16, 128, 112));
}

#[test]
fn la_decision_blanda_corrige_un_error_como_la_dura() {
    for (d, w) in palabras() {
        assert_eq!(decodificar_blando(&blandos_de(w, 10)), d);
        for i in 0..8 {
            assert_eq!(decodificar_blando(&blandos_de(w ^ (1 << i), 10)), d);
        }
    }
}

#[test]
fn la_decision_blanda_recupera_dos_errores_dudosos() {
    // Dos bits llegan al revés pero apenas (magnitud 1), y los otros seis claros (10). La
    // decisión dura solo puede decir «doble error»; la blanda, que pesa la duda, acierta.
    for (d, w) in palabras() {
        for i in 0..8 {
            for j in i + 1..8 {
                let mut blandos = blandos_de(w, 10);
                blandos[i] = -blandos[i].signum();
                blandos[j] = -blandos[j].signum();
                assert_eq!(
                    decodificar(w ^ (1 << i) ^ (1 << j)),
                    Decodificacion::DobleError
                );
                assert_eq!(decodificar_blando(&blandos), d, "dato {d}, bits {i} y {j}");
            }
        }
    }
}

#[test]
fn la_decision_blanda_no_desborda_con_valores_extremos() {
    assert_eq!(decodificar_blando(&[i64::MAX; 8]), 0);
    assert_eq!(decodificar_blando(&[i64::MIN; 8]), 15);
    // En empate total gana el dato menor.
    assert_eq!(decodificar_blando(&[0; 8]), 0);
}
