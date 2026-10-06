//! La cadena completa —Hamming, entrelazado, esparcido, preámbulo, canal, sincronía y
//! vuelta—, la comparación entre decisión dura y blanda, y la medición de la ganancia de
//! procesamiento.

mod canal;

use canal::{
    armar_trama, bits_de_las_lecturas, ganancia_db, leer_cuerpo, leer_trama, rafaga_de_ruido,
    rafaga_invertida, tono_tolerado, Azar, Canal,
};
use spread_spectrum_codec::ensanchado::esparcir;
use spread_spectrum_codec::gold::ParPreferente;
use spread_spectrum_codec::hamming::Decodificacion;
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};

const A: i32 = 1000;

fn preambulo() -> Codigo {
    Codigo::secuencia_m(Polinomio::GRADO_7)
}

fn recuperada(decodificacion: Decodificacion, lectura: u8) -> bool {
    matches!(
        decodificacion,
        Decodificacion::Intacta(x) | Decodificacion::Corregida(x) if x == lectura
    )
}

#[test]
fn ida_y_vuelta_sin_ruido_por_toda_la_cadena() {
    let codigos = [
        Codigo::secuencia_m(Polinomio::GRADO_3),
        ParPreferente::DE_31.codigo(4).unwrap(),
        Codigo::secuencia_m(Polinomio::GRADO_6),
    ];
    let lecturas: Vec<u8> = (0..16).collect();
    for codigo in codigos {
        let trama = armar_trama(&lecturas, &codigo, &preambulo(), A);
        let r = leer_trama(&trama, lecturas.len(), &codigo, &preambulo(), A).expect("trama");
        assert_eq!(r.posicion, 0);
        let intactas: Vec<_> = lecturas
            .iter()
            .map(|&l| Decodificacion::Intacta(l))
            .collect();
        assert_eq!(r.cuerpo.duras, intactas, "N = {}", codigo.largo());
        assert_eq!(r.cuerpo.blandas, lecturas);
    }
}

#[test]
fn una_rafaga_de_d_bits_con_ruido_y_tono_se_corrige_entera() {
    // 16 lecturas (profundidad de entrelazado D = 16) con un código Gold de 31 chips, 333
    // muestras después de empezar a escuchar. Una ráfaga invierte 16 bits seguidos del cuerpo
    // desde el bit 37, y el canal suma ruido de σ = A por chip y un tono del doble de la
    // amplitud de chip.
    let codigo = ParPreferente::DE_31.codigo(5).unwrap();
    let pre = preambulo();
    let lecturas: Vec<u8> = (0..16u8).map(|i| (i * 5 + 3) % 16).collect();
    let trama = armar_trama(&lecturas, &codigo, &pre, A);
    let antes = 333;
    let mut aire = vec![0i32; antes + trama.len() + 200];
    aire[antes..antes + trama.len()].copy_from_slice(&trama);
    let n = codigo.largo();
    let cuerpo = antes + pre.largo();
    rafaga_invertida(&mut aire, cuerpo + 37 * n..cuerpo + (37 + 16) * n);
    Canal::nuevo(40, i64::from(A), 2 * i64::from(A)).pasar(&mut aire);

    let r = leer_trama(&aire, lecturas.len(), &codigo, &pre, A).expect("trama");
    assert_eq!(r.posicion, antes);
    // La ráfaga tocó cada palabra exactamente una vez, y nada más falló: las 16 lecturas
    // llegan corregidas, con las dos decisiones.
    let corregidas: Vec<_> = lecturas
        .iter()
        .map(|&l| Decodificacion::Corregida(l))
        .collect();
    assert_eq!(r.cuerpo.duras, corregidas);
    assert_eq!(r.cuerpo.blandas, lecturas);
}

#[test]
fn con_ruido_gaussiano_la_decision_blanda_recupera_mas_lecturas_que_la_dura() {
    // 400 tramas de 8 lecturas con N = 31 y ruido fuerte: σ = 3,4 · A por chip, cerca de un
    // 5 % de bits volteados. Sin tono y sin buscar el preámbulo (el cuerpo empieza donde se
    // sabe): solo se compara cómo decide cada una.
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);
    let mut azar = Azar::nuevo(50);
    let mut canal = Canal::nuevo(51, 34 * i64::from(A) / 10, 0);
    let (mut duras, mut blandas, mut total) = (0, 0, 0);
    for _ in 0..400 {
        let lecturas: Vec<u8> = (0..8).map(|_| azar.hasta(16) as u8).collect();
        let bits = bits_de_las_lecturas(&lecturas);
        let mut muestras = vec![0i32; bits.len() * codigo.largo()];
        esparcir(&bits, &codigo, A, &mut muestras).unwrap();
        canal.pasar(&mut muestras);
        let cuerpo = leer_cuerpo(&muestras, lecturas.len(), &codigo, Some(A));
        for ((&l, &d), &b) in lecturas.iter().zip(&cuerpo.duras).zip(&cuerpo.blandas) {
            duras += usize::from(recuperada(d, l));
            blandas += usize::from(b == l);
            total += 1;
        }
    }
    println!("de {total} lecturas: {duras} con decisión dura, {blandas} con blanda");
    assert!(blandas > duras);
}

#[test]
fn recortar_los_blandos_salva_a_la_decision_blanda_de_una_rafaga_enorme() {
    // Una ráfaga de ruido de σ = 30 · A sobre seis bits: sin recorte, cada bit tapado pesa
    // varias veces lo que un bit limpio y arrastra a su palabra; recortado a N · A, pesa
    // como uno más y la palabra se corrige. La decisión dura no lo necesita.
    let codigo = ParPreferente::DE_31.codigo(3).unwrap();
    let lecturas = [7u8, 8, 8, 9, 11, 12, 12, 10];
    let bits = bits_de_las_lecturas(&lecturas);
    let n = codigo.largo();
    let mut muestras = vec![0i32; bits.len() * n];
    esparcir(&bits, &codigo, A, &mut muestras).unwrap();
    Canal::nuevo(70, i64::from(A), 0).pasar(&mut muestras);
    rafaga_de_ruido(
        &mut muestras,
        20 * n..26 * n,
        30 * i64::from(A),
        &mut Azar::nuevo(71),
    );

    let sin_recorte = leer_cuerpo(&muestras, lecturas.len(), &codigo, None);
    let con_recorte = leer_cuerpo(&muestras, lecturas.len(), &codigo, Some(A));
    let volteados = bits
        .iter()
        .zip(&con_recorte.bits)
        .filter(|(a, b)| a != b)
        .count();
    assert!(volteados > 0, "la ráfaga tenía que voltear algún bit");
    assert!(con_recorte
        .duras
        .iter()
        .zip(&lecturas)
        .all(|(&d, &l)| recuperada(d, l)));
    assert_eq!(con_recorte.blandas, lecturas);
    assert_ne!(sin_recorte.blandas, lecturas);
}

#[test]
fn una_trama_cortada_no_se_lee() {
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_4);
    let lecturas = [1u8, 2, 3];
    let trama = armar_trama(&lecturas, &codigo, &preambulo(), A);
    let cortada = &trama[..trama.len() - 1];
    assert!(leer_trama(cortada, lecturas.len(), &codigo, &preambulo(), A).is_none());
    assert!(leer_trama(&trama, lecturas.len(), &codigo, &preambulo(), A).is_some());
}

/// La versión liviana de `examples/ganancia.rs`: la misma amplitud, el mismo ruido y las
/// mismas semillas, con menos bits por intento.
const SEMILLA: u64 = 2026;

#[test]
fn con_ruido_la_ganancia_medida_crece_con_n() {
    const SIGMA: i64 = 100;
    const BITS: usize = 20_000;
    let referencia = tono_tolerado(None, A, SIGMA, BITS, SEMILLA);
    let mut anterior = referencia;
    for p in Polinomio::POR_GRADO {
        let codigo = Codigo::secuencia_m(p);
        let n = codigo.largo();
        let tolerado = tono_tolerado(Some(&codigo), A, SIGMA, BITS, SEMILLA);
        let medida = ganancia_db(tolerado, referencia);
        println!("N = {n}: tono {tolerado}, medida {medida:.2} dB");
        assert!(tolerado > anterior, "N = {n} no tolera más que el anterior");
        anterior = tolerado;
    }
}

#[test]
fn sin_ruido_el_tono_tolerado_es_el_de_la_tabla_y_queda_a_menos_de_1_5_db_de_10_log_n() {
    // Sin ruido, el error lo decide solo el tono: el umbral es la peor combinación de fase y
    // signo de bit, y no depende de cuántos bits se miren. Con 2 000 bits sale exactamente lo
    // que el ejemplo mide con 200 000 (la tabla del README).
    const BITS: usize = 2_000;
    const TABLA: [(Polinomio, i64); 5] = [
        (Polinomio::GRADO_3, 2678),
        (Polinomio::GRADO_4, 4065),
        (Polinomio::GRADO_5, 6458),
        (Polinomio::GRADO_6, 8002),
        (Polinomio::GRADO_7, 10207),
    ];
    let referencia = tono_tolerado(None, A, 0, BITS, SEMILLA);
    assert_eq!(referencia, i64::from(A) - 1);
    for (p, esperado) in TABLA {
        let codigo = Codigo::secuencia_m(p);
        let n = codigo.largo();
        let tolerado = tono_tolerado(Some(&codigo), A, 0, BITS, SEMILLA);
        let medida = ganancia_db(tolerado, referencia);
        let teoria = 10.0 * (n as f64).log10();
        assert_eq!(tolerado, esperado, "N = {n}");
        assert!(
            (medida - teoria).abs() < 1.5,
            "N = {n}: medida {medida:.2} dB, teoría {teoria:.2} dB"
        );
    }
}
