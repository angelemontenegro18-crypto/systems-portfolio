//! Los códigos Gold: para cualquier par de la familia y cualquier desfase (salvo un código
//! consigo mismo sin desfase), la correlación toma solo los tres valores de la teoría.

use std::collections::BTreeSet;

use spread_spectrum_codec::gold::ParPreferente;
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};

/// Todas las correlaciones de la familia de `par`, con `i ≤ j` (la de `j` con `i` es la misma
/// leída al revés). Devuelve los valores que aparecieron.
fn correlaciones_de_la_familia(par: ParPreferente) -> BTreeSet<i32> {
    let familia: Vec<Vec<i32>> = (0..par.tamano_de_familia())
        .map(|i| {
            let codigo = par.codigo(i).expect("índice dentro de la familia");
            codigo.chips().iter().map(|&c| i32::from(c)).collect()
        })
        .collect();
    let n = par.largo();
    let mut vistos = BTreeSet::new();
    for (i, a) in familia.iter().enumerate() {
        for b in &familia[i..] {
            for desfase in 0..n {
                if std::ptr::eq(a, b) && desfase == 0 {
                    continue;
                }
                let r: i32 = (0..n).map(|k| a[k] * b[(k + desfase) % n]).sum();
                vistos.insert(r);
            }
        }
    }
    vistos
}

fn comprobar(par: ParPreferente, esperados: [i32; 3]) {
    assert_eq!(par.valores_de_correlacion(), esperados);
    let vistos = correlaciones_de_la_familia(par);
    let teoricos: BTreeSet<i32> = esperados.into_iter().collect();
    // Solo los tres valores, y los tres aparecen: la cota es justa.
    assert_eq!(vistos, teoricos, "largo {}", par.largo());
}

#[test]
fn familia_de_31_tres_valores_exhaustivo() {
    comprobar(ParPreferente::DE_31, [-1, -9, 7]);
}

#[test]
fn familia_de_63_tres_valores_exhaustivo() {
    comprobar(ParPreferente::DE_63, [-1, -17, 15]);
}

#[test]
fn familia_de_127_tres_valores_exhaustivo() {
    comprobar(ParPreferente::DE_127, [-1, -17, 15]);
}

#[test]
fn la_familia_tiene_n_mas_dos_codigos_distintos() {
    for par in [
        ParPreferente::DE_31,
        ParPreferente::DE_63,
        ParPreferente::DE_127,
    ] {
        let n = par.largo();
        assert_eq!(par.tamano_de_familia(), n + 2);
        let familia: Vec<Codigo> = (0..par.tamano_de_familia())
            .map(|i| par.codigo(i).unwrap())
            .collect();
        for (i, a) in familia.iter().enumerate() {
            assert_eq!(a.largo(), n);
            for b in &familia[i + 1..] {
                assert_ne!(a.chips(), b.chips());
            }
        }
        assert_eq!(par.codigo(par.tamano_de_familia()), None);
    }
}

#[test]
fn los_dos_primeros_son_las_secuencias_del_par() {
    let par = ParPreferente::DE_31;
    assert_eq!(par.grado(), 5);
    assert_eq!(par.codigo(0), Some(Codigo::secuencia_m(Polinomio::GRADO_5)));
    assert_eq!(
        par.codigo(1),
        Some(Codigo::secuencia_m(Polinomio::GRADO_5_B))
    );
}

#[test]
fn una_secuencia_m_y_su_inversa_no_forman_par_preferente() {
    // Control negativo: la cota de tres valores es propiedad del par preferente, no de dos
    // secuencias m cualesquiera. La secuencia de `x⁵ + x² + 1` leída al revés es la de
    // `x⁵ + x³ + 1`, y con ella la correlación cruzada se sale de la cota.
    let a: Vec<i32> = Codigo::secuencia_m(Polinomio::GRADO_5)
        .chips()
        .iter()
        .map(|&c| i32::from(c))
        .collect();
    let b: Vec<i32> = a.iter().rev().copied().collect();
    let n = a.len();
    let maximo = (0..n)
        .map(|d| (0..n).map(|k| a[k] * b[(k + d) % n]).sum::<i32>().abs())
        .max()
        .unwrap();
    assert!(maximo > 9, "máximo {maximo}");
}
