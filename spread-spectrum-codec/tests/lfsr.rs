//! Las tres propiedades de las secuencias de máximo largo, comprobadas **para cada grado** de
//! la tabla (3 a 7, y los tres compañeros de los pares preferentes): periodo `2ⁿ − 1`,
//! equilibrio y autocorrelación de dos valores.

use std::collections::HashSet;

use spread_spectrum_codec::lfsr::{
    chip_de, correlacion_periodica, Codigo, Lfsr, Polinomio, LARGO_MAXIMO,
};
use spread_spectrum_codec::Error;

/// Los polinomios de la tabla como listas de exponentes —la notación de las tablas
/// publicadas—, escritos aquí a mano y no leídos de la biblioteca: es la referencia
/// independiente contra la que se comprueba la máscara de realimentación.
const EXPONENTES: [(Polinomio, &[usize]); 8] = [
    (Polinomio::GRADO_3, &[3, 1, 0]),
    (Polinomio::GRADO_4, &[4, 1, 0]),
    (Polinomio::GRADO_5, &[5, 2, 0]),
    (Polinomio::GRADO_6, &[6, 1, 0]),
    (Polinomio::GRADO_7, &[7, 3, 0]),
    (Polinomio::GRADO_5_B, &[5, 4, 3, 2, 0]),
    (Polinomio::GRADO_6_B, &[6, 5, 2, 1, 0]),
    (Polinomio::GRADO_7_B, &[7, 3, 2, 1, 0]),
];

#[test]
fn la_tabla_tiene_un_polinomio_por_grado_de_3_a_7() {
    let grados: Vec<u8> = Polinomio::POR_GRADO.iter().map(|p| p.grado()).collect();
    assert_eq!(grados, [3, 4, 5, 6, 7]);
    assert_eq!(Polinomio::TODOS.len(), EXPONENTES.len());
    for (p, _) in EXPONENTES {
        assert!(Polinomio::TODOS.contains(&p));
    }
    assert_eq!(Polinomio::GRADO_7.periodo(), LARGO_MAXIMO);
}

#[test]
fn la_salida_cumple_la_recurrencia_del_polinomio() {
    for (p, exponentes) in EXPONENTES {
        let n = exponentes[0];
        assert_eq!(usize::from(p.grado()), n);
        let mut registro = Lfsr::nuevo(p);
        let salida: Vec<u8> = (0..3 * p.periodo()).map(|_| registro.siguiente()).collect();
        // xⁿ = Σ xᵏ: cada bit nuevo es el XOR de los que marcan los demás exponentes.
        for t in 0..salida.len() - n {
            let esperado = exponentes[1..].iter().fold(0, |x, &k| x ^ salida[t + k]);
            assert_eq!(salida[t + n], esperado, "grado {n}, paso {t}");
        }
    }
}

#[test]
fn el_periodo_es_dos_a_la_n_menos_uno_en_cada_grado() {
    for p in Polinomio::TODOS {
        let mut registro = Lfsr::nuevo(p);
        let inicial = registro.estado();
        let mut vistos = HashSet::new();
        let mut pasos = 0usize;
        loop {
            assert_ne!(registro.estado(), 0, "grado {}: estado nulo", p.grado());
            assert!(
                vistos.insert(registro.estado()),
                "grado {}: un estado se repitió antes de volver al inicial",
                p.grado()
            );
            registro.siguiente();
            pasos += 1;
            if registro.estado() == inicial {
                break;
            }
            assert!(pasos < 1 << p.grado(), "grado {}: no vuelve", p.grado());
        }
        assert_eq!(pasos, (1 << p.grado()) - 1, "grado {}", p.grado());
        assert_eq!(pasos, p.periodo());
    }
}

#[test]
fn en_un_periodo_hay_un_uno_mas_que_ceros() {
    for p in Polinomio::TODOS {
        let codigo = Codigo::secuencia_m(p);
        let unos = codigo.chips().iter().filter(|&&c| c == chip_de(1)).count();
        let ceros = codigo.chips().iter().filter(|&&c| c == chip_de(0)).count();
        assert_eq!(unos + ceros, p.periodo(), "grado {}", p.grado());
        assert_eq!(unos, ceros + 1, "grado {}", p.grado());
        assert_eq!(unos, 1 << (p.grado() - 1));
    }
}

#[test]
fn la_autocorrelacion_vale_n_sin_desfase_y_menos_uno_con_cualquier_otro() {
    for p in Polinomio::TODOS {
        let codigo = Codigo::secuencia_m(p);
        let n = codigo.largo() as i32;
        for desfase in 0..codigo.largo() {
            let r = correlacion_periodica(&codigo, &codigo, desfase).unwrap();
            let esperado = if desfase == 0 { n } else { -1 };
            assert_eq!(r, esperado, "grado {}, desfase {desfase}", p.grado());
        }
        // El desfase es cíclico.
        assert_eq!(
            correlacion_periodica(&codigo, &codigo, codigo.largo()),
            Ok(n)
        );
    }
}

#[test]
fn la_secuencia_se_repite_con_su_periodo() {
    for p in Polinomio::TODOS {
        let mut registro = Lfsr::nuevo(p);
        let salida: Vec<u8> = (0..2 * p.periodo()).map(|_| registro.siguiente()).collect();
        let (primera, segunda) = salida.split_at(p.periodo());
        assert_eq!(primera, segunda, "grado {}", p.grado());
        let chips: Vec<i8> = primera.iter().map(|&b| chip_de(b)).collect();
        assert_eq!(Codigo::secuencia_m(p).chips(), chips.as_slice());
    }
}

#[test]
fn los_chips_son_mas_y_menos_uno() {
    assert_eq!(chip_de(0), 1);
    assert_eq!(chip_de(1), -1);
    for p in Polinomio::TODOS {
        assert!(Codigo::secuencia_m(p)
            .chips()
            .iter()
            .all(|&c| c == 1 || c == -1));
    }
}

#[test]
fn correlar_codigos_de_largo_distinto_es_un_error() {
    let a = Codigo::secuencia_m(Polinomio::GRADO_3);
    let b = Codigo::secuencia_m(Polinomio::GRADO_4);
    assert_eq!(
        correlacion_periodica(&a, &b, 0),
        Err(Error::LargosDistintos)
    );
    assert_eq!(a.producto_desplazado(&b, 0), Err(Error::LargosDistintos));
}
