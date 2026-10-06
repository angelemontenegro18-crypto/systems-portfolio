//! Esparcir y desesparcir: ida y vuelta, el valor exacto de la correlación, la tolerancia
//! exacta a un desplazamiento constante, tres sensores en el mismo canal, y los errores de
//! entrada.

mod canal;

use canal::{Azar, Canal};
use spread_spectrum_codec::ensanchado::{correlar, decidir, esparcir, recortar};
use spread_spectrum_codec::gold::ParPreferente;
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};
use spread_spectrum_codec::Error;

const A: i32 = 100;

/// Las cinco secuencias m, y tres códigos de cada familia Gold.
fn codigos() -> Vec<Codigo> {
    let mut codigos: Vec<Codigo> = Polinomio::POR_GRADO
        .iter()
        .map(|&p| Codigo::secuencia_m(p))
        .collect();
    for par in [
        ParPreferente::DE_31,
        ParPreferente::DE_63,
        ParPreferente::DE_127,
    ] {
        for i in [1, 2, par.tamano_de_familia() - 1] {
            codigos.push(par.codigo(i).unwrap());
        }
    }
    codigos
}

fn bits_al_azar(azar: &mut Azar, cuantos: usize) -> Vec<u8> {
    (0..cuantos).map(|_| azar.bit()).collect()
}

#[test]
fn ida_y_vuelta_sin_ruido_con_cada_codigo() {
    let mut azar = Azar::nuevo(1);
    for codigo in codigos() {
        let bits = bits_al_azar(&mut azar, 200);
        let total = bits.len() * codigo.largo();
        // La salida sobra en 3 muestras: solo se escribe el principio.
        let mut muestras = vec![7i32; total + 3];
        assert_eq!(esparcir(&bits, &codigo, A, &mut muestras), Ok(total));
        assert_eq!(muestras[total..], [7, 7, 7]);
        let mut vuelta = vec![9u8; bits.len()];
        assert_eq!(
            decidir(&muestras[..total], &codigo, &mut vuelta),
            Ok(bits.len())
        );
        assert_eq!(vuelta, bits, "N = {}", codigo.largo());
    }
}

#[test]
fn la_correlacion_de_un_bit_vale_n_por_la_amplitud_con_su_signo() {
    for codigo in codigos() {
        let n = codigo.largo() as i64;
        let mut muestras = vec![0i32; 2 * codigo.largo()];
        esparcir(&[0, 1], &codigo, A, &mut muestras).unwrap();
        let mut blandos = [0i64; 2];
        assert_eq!(correlar(&muestras, &codigo, &mut blandos), Ok(2));
        assert_eq!(blandos, [n * i64::from(A), -n * i64::from(A)]);
    }
}

#[test]
fn un_desplazamiento_constante_se_tolera_hasta_n_por_la_amplitud() {
    // Una secuencia m suma −1, así que un desplazamiento D le resta D a la correlación de un
    // 0: el bit aguanta hasta D = N · A (correlación cero, que se decide como 0) y ni una
    // unidad más. Sin esparcir, el mismo bit caería con D = A + 1.
    for p in Polinomio::POR_GRADO {
        let codigo = Codigo::secuencia_m(p);
        let limite = codigo.largo() as i32 * A;
        for (desplazamiento, esperado) in [(limite, 0u8), (limite + 1, 1u8)] {
            let mut muestras = vec![0i32; codigo.largo()];
            esparcir(&[0], &codigo, A, &mut muestras).unwrap();
            for m in &mut muestras {
                *m += desplazamiento;
            }
            let mut bit = [9u8];
            decidir(&muestras, &codigo, &mut bit).unwrap();
            assert_eq!(
                bit,
                [esperado],
                "N = {}, D = {desplazamiento}",
                codigo.largo()
            );
        }
    }
}

#[test]
fn la_decision_dura_es_el_signo_de_la_blanda() {
    let mut azar = Azar::nuevo(2);
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);
    let bits = bits_al_azar(&mut azar, 2000);
    let mut muestras = vec![0i32; bits.len() * codigo.largo()];
    esparcir(&bits, &codigo, A, &mut muestras).unwrap();
    // Ruido fuerte (σ = 4A por chip) para que haya de todo, errores incluidos.
    Canal::nuevo(3, 4 * i64::from(A), 0).pasar(&mut muestras);
    let mut blandos = vec![0i64; bits.len()];
    let mut duros = vec![0u8; bits.len()];
    correlar(&muestras, &codigo, &mut blandos).unwrap();
    decidir(&muestras, &codigo, &mut duros).unwrap();
    for (blando, duro) in blandos.iter().zip(&duros) {
        assert_eq!(*duro, u8::from(*blando < 0));
    }
    let errores = bits.iter().zip(&duros).filter(|(a, b)| a != b).count();
    assert!(errores > 0, "con este ruido algún bit tenía que caer");
}

#[test]
fn tres_sensores_comparten_el_canal_con_codigos_gold() {
    // Tres sensores transmiten a la vez y alineados, con tres códigos de la familia de 31.
    // Cada receptor correla con el suyo: la correlación cruzada (a lo sumo 9 por bit, contra
    // 31) no alcanza a voltear ningún bit, ni sumando la de los otros dos.
    let par = ParPreferente::DE_31;
    let codigos: Vec<Codigo> = [0, 2, 20].iter().map(|&i| par.codigo(i).unwrap()).collect();
    let mut azar = Azar::nuevo(4);
    let bits: Vec<Vec<u8>> = codigos
        .iter()
        .map(|_| bits_al_azar(&mut azar, 500))
        .collect();
    let mut aire = vec![0i32; 500 * par.largo()];
    for (codigo, propios) in codigos.iter().zip(&bits) {
        let mut senal = vec![0i32; aire.len()];
        esparcir(propios, codigo, A, &mut senal).unwrap();
        for (x, y) in aire.iter_mut().zip(&senal) {
            *x += y;
        }
    }
    for (codigo, propios) in codigos.iter().zip(&bits) {
        let mut recibidos = vec![0u8; 500];
        decidir(&aire, codigo, &mut recibidos).unwrap();
        assert_eq!(&recibidos, propios);
    }
}

#[test]
fn recortar_limita_cada_valor_al_tope_sin_cambiar_su_signo() {
    let mut blandos = [i64::MIN, -5000, -3100, -1, 0, 1, 3100, 5000, i64::MAX];
    recortar(&mut blandos, 3100);
    assert_eq!(blandos, [-3100, -3100, -3100, -1, 0, 1, 3100, 3100, 3100]);
    // Un tope negativo cuenta por su valor absoluto, y el más negativo no desborda.
    let mut otra = [-9i64, 9];
    recortar(&mut otra, -4);
    assert_eq!(otra, [-4, 4]);
    recortar(&mut otra, i64::MIN);
    assert_eq!(otra, [-4, 4]);
}

#[test]
fn las_entradas_invalidas_se_rechazan_sin_escribir() {
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_3);
    let mut muestras = [5i32; 14];
    assert_eq!(
        esparcir(&[0, 2], &codigo, A, &mut muestras),
        Err(Error::BitInvalido)
    );
    assert_eq!(
        esparcir(&[0, 1, 0], &codigo, A, &mut muestras),
        Err(Error::SalidaCorta)
    );
    assert_eq!(muestras, [5; 14]);

    let mut blandos = [0i64; 2];
    assert_eq!(
        correlar(&muestras[..13], &codigo, &mut blandos),
        Err(Error::LargoNoMultiplo)
    );
    assert_eq!(
        correlar(&muestras, &codigo, &mut blandos[..1]),
        Err(Error::SalidaCorta)
    );
    let mut bits = [0u8; 2];
    assert_eq!(
        decidir(&muestras[..13], &codigo, &mut bits),
        Err(Error::LargoNoMultiplo)
    );
    assert_eq!(
        decidir(&muestras, &codigo, &mut bits[..1]),
        Err(Error::SalidaCorta)
    );
    assert_eq!((blandos, bits), ([0, 0], [0, 0]));

    // Sin muestras no hay bits, y no es un error.
    assert_eq!(decidir(&[], &codigo, &mut bits), Ok(0));
    assert_eq!(esparcir(&[], &codigo, A, &mut []), Ok(0));
}

#[test]
fn la_amplitud_extrema_satura_en_vez_de_desbordar() {
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_3);
    let mut muestras = [0i32; 7];
    esparcir(&[1], &codigo, i32::MIN, &mut muestras).unwrap();
    assert!(muestras.iter().all(|&m| m == i32::MAX || m == i32::MIN));
    let mut bit = [9u8];
    decidir(&muestras, &codigo, &mut bit).unwrap();
    // Con amplitud negativa el bit sale invertido, pero sin pánico.
    assert_eq!(bit, [0]);
}
