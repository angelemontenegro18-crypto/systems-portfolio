//! La sincronización de trama: el pico de correlación cae exactamente donde empieza el
//! preámbulo, y el umbral de media altura separa una trama presente de una ausente.

mod canal;

use canal::{rafaga_de_ruido, Azar, Canal};
use spread_spectrum_codec::ensanchado::esparcir;
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};
use spread_spectrum_codec::sincronia::{buscar_preambulo, umbral_de_media_altura, Pico};

const A: i32 = 100;

/// El preámbulo de las pruebas: la secuencia m de grado 7, 127 chips.
fn preambulo() -> Codigo {
    Codigo::secuencia_m(Polinomio::GRADO_7)
}

/// `antes` muestras en silencio, el preámbulo a `amplitud`, y `datos` bits al azar a la
/// amplitud nominal, esparcidos con la secuencia de grado 5.
fn senal(antes: usize, amplitud: i32, datos: usize, azar: &mut Azar) -> Vec<i32> {
    let pre = preambulo();
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);
    let mut v = vec![0i32; antes + pre.largo() + datos * codigo.largo()];
    esparcir(&[0], &pre, amplitud, &mut v[antes..antes + pre.largo()]).unwrap();
    let bits: Vec<u8> = (0..datos).map(|_| azar.bit()).collect();
    esparcir(&bits, &codigo, A, &mut v[antes + pre.largo()..]).unwrap();
    v
}

/// La correlación más alta con el preámbulo en todas las posiciones, calculada aquí y no con
/// la biblioteca (que se detiene en el primer evento).
fn maximo(muestras: &[i32]) -> i64 {
    let chips = preambulo().chips().to_vec();
    muestras
        .windows(chips.len())
        .map(|ventana| {
            ventana
                .iter()
                .zip(&chips)
                .map(|(&m, &c)| i64::from(m) * i64::from(c))
                .sum::<i64>()
        })
        .max()
        .unwrap_or(i64::MIN)
}

#[test]
fn el_pico_cae_donde_empieza_el_preambulo() {
    let mut azar = Azar::nuevo(10);
    let pre = preambulo();
    let umbral = umbral_de_media_altura(&pre, A);
    for antes in [0, 1, 2, 50, 127, 300, 1001] {
        let mut v = senal(antes, A, 20, &mut azar);
        // Ruido de σ = A por chip.
        Canal::nuevo(antes as u64, i64::from(A), 0).pasar(&mut v);
        let pico = buscar_preambulo(&v, &pre, umbral).expect("hay preámbulo");
        assert_eq!(pico.posicion, antes);
    }
}

#[test]
fn sin_preambulo_ni_el_ruido_ni_los_datos_llegan_a_media_altura() {
    let umbral = umbral_de_media_altura(&preambulo(), A);

    let mut ruido = vec![0i32; 5000];
    Canal::nuevo(20, i64::from(A), 0).pasar(&mut ruido);
    let con_ruido = maximo(&ruido);

    let mut azar = Azar::nuevo(21);
    let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);
    let bits: Vec<u8> = (0..200).map(|_| azar.bit()).collect();
    let mut datos = vec![0i32; bits.len() * codigo.largo()];
    esparcir(&bits, &codigo, A, &mut datos).unwrap();
    let con_datos = maximo(&datos);

    assert!(
        con_ruido < umbral && con_datos < umbral,
        "ruido {con_ruido}, datos {con_datos}, umbral {umbral}"
    );
    assert_eq!(buscar_preambulo(&ruido, &preambulo(), umbral), None);
    assert_eq!(buscar_preambulo(&datos, &preambulo(), umbral), None);
}

#[test]
fn el_umbral_de_media_altura_es_la_mitad_del_pico_completo() {
    assert_eq!(umbral_de_media_altura(&preambulo(), A), 127 * 100 / 2);
    let corto = Codigo::secuencia_m(Polinomio::GRADO_3);
    assert_eq!(umbral_de_media_altura(&corto, 10), 35);
    // Sin ruido, el pico completo vale exactamente el doble del umbral.
    let mut azar = Azar::nuevo(22);
    let v = senal(9, A, 0, &mut azar);
    assert_eq!(
        buscar_preambulo(&v, &preambulo(), 0),
        Some(Pico {
            posicion: 9,
            correlacion: 2 * umbral_de_media_altura(&preambulo(), A),
        })
    );
}

#[test]
fn el_umbral_separa_una_trama_al_cuarenta_por_ciento_de_una_al_sesenta() {
    // Sin ruido, el pico vale L · amplitud recibida: a 40 % de la amplitud no llega a media
    // altura y no hay trama; a 60 % sí, y en su lugar.
    let pre = preambulo();
    let umbral = umbral_de_media_altura(&pre, A);
    let mut azar = Azar::nuevo(30);
    let debil = senal(40, A * 4 / 10, 10, &mut azar);
    assert_eq!(buscar_preambulo(&debil, &pre, umbral), None);
    let suficiente = senal(40, A * 6 / 10, 10, &mut azar);
    assert_eq!(
        buscar_preambulo(&suficiente, &pre, umbral).map(|p| p.posicion),
        Some(40)
    );
}

#[test]
fn una_rafaga_fuerte_despues_del_preambulo_no_lo_desplaza() {
    // Una ráfaga de ruido de σ = 30 · A sobre los datos correla con el preámbulo mucho más que
    // el preámbulo mismo; como manda la primera posición que alcanza el umbral, no importa.
    let pre = preambulo();
    let umbral = umbral_de_media_altura(&pre, A);
    let mut azar = Azar::nuevo(61);
    let mut v = senal(50, A, 40, &mut azar);
    let datos = 50 + pre.largo();
    rafaga_de_ruido(
        &mut v,
        datos + 300..datos + 500,
        30 * i64::from(A),
        &mut azar,
    );
    let en_la_rafaga = maximo(&v[datos..]);
    assert!(en_la_rafaga > 127 * i64::from(A), "{en_la_rafaga}");
    assert_eq!(
        buscar_preambulo(&v, &pre, umbral).map(|p| p.posicion),
        Some(50)
    );
}

#[test]
fn una_rafaga_fuerte_antes_del_preambulo_engana_a_la_busqueda() {
    // El límite: si la ráfaga llega antes, la primera posición que alcanza el umbral es suya.
    let pre = preambulo();
    let umbral = umbral_de_media_altura(&pre, A);
    let mut azar = Azar::nuevo(62);
    let mut v = senal(600, A, 10, &mut azar);
    rafaga_de_ruido(&mut v, 100..300, 30 * i64::from(A), &mut azar);
    let pico = buscar_preambulo(&v, &pre, umbral).expect("algo alcanza el umbral");
    assert!(pico.posicion < 300, "posición {}", pico.posicion);
}

#[test]
fn en_empate_gana_la_primera_posicion() {
    let pre = preambulo();
    let mut v = vec![0i32; 3 + 2 * pre.largo()];
    esparcir(&[0, 0], &pre, A, &mut v[3..]).unwrap();
    assert_eq!(
        buscar_preambulo(&v, &pre, 0),
        Some(Pico {
            posicion: 3,
            correlacion: 127 * 100,
        })
    );
}

#[test]
fn con_menos_muestras_que_el_preambulo_no_hay_pico() {
    let pre = preambulo();
    assert_eq!(buscar_preambulo(&[100; 126], &pre, i64::MIN), None);
    assert_eq!(buscar_preambulo(&[], &pre, i64::MIN), None);
    assert!(buscar_preambulo(&[100; 127], &pre, i64::MIN).is_some());
}
