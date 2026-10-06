//! Un enlace de la red de sensores de la planta.
//!
//! El sensor de nivel del tanque 3 manda ocho lecturas (de 0 a 15) con su código Gold de 31
//! chips. El canal agrega ruido, un tono del doble de la amplitud de la señal —el variador de
//! una bomba, mal filtrado— y una ráfaga que tapa seis bits seguidos. El receptor busca el preámbulo,
//! desesparce, desentrelaza y decodifica, con decisión dura y con decisión blanda.
//!
//! ```text
//! cargo run --example enlace
//! ```

#[path = "../tests/canal/mod.rs"]
mod canal;

use canal::{armar_trama, bits_de_las_lecturas, leer_trama, rafaga_de_ruido, Azar, Canal};
use spread_spectrum_codec::gold::ParPreferente;
use spread_spectrum_codec::hamming::Decodificacion;
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};

const AMPLITUD: i32 = 1000;
const SEMILLA: u64 = 3;

fn main() {
    let preambulo = Codigo::secuencia_m(Polinomio::GRADO_7);
    let codigo = ParPreferente::DE_31
        .codigo(3)
        .expect("la familia de 31 tiene 33 códigos");
    let lecturas = [7u8, 8, 8, 9, 11, 12, 12, 10];

    let trama = armar_trama(&lecturas, &codigo, &preambulo, AMPLITUD);
    let antes = 500;
    let mut aire = vec![0i32; antes + trama.len() + 300];
    aire[antes..antes + trama.len()].copy_from_slice(&trama);

    let sigma = i64::from(AMPLITUD);
    let tono = 2 * i64::from(AMPLITUD);
    Canal::nuevo(SEMILLA, sigma, tono).pasar(&mut aire);
    let n = codigo.largo();
    let cuerpo = antes + preambulo.largo();
    let rafaga = cuerpo + 20 * n..cuerpo + 26 * n;
    rafaga_de_ruido(
        &mut aire,
        rafaga.clone(),
        30 * sigma,
        &mut Azar::nuevo(SEMILLA + 10),
    );

    println!(
        "trama: {} chips de preámbulo + {} bits × {n} chips, desde la muestra {antes}",
        preambulo.largo(),
        8 * lecturas.len()
    );
    println!(
        "canal: ruido σ = {sigma} y tono de amplitud {tono} por chip (la señal: {AMPLITUD}), \
         ráfaga sobre los bits 20 a 25"
    );

    let Some(recepcion) = leer_trama(&aire, lecturas.len(), &codigo, &preambulo, AMPLITUD) else {
        println!("no se encontró el preámbulo");
        return;
    };
    let enviados = bits_de_las_lecturas(&lecturas);
    let errores = enviados
        .iter()
        .zip(&recepcion.cuerpo.bits)
        .filter(|(a, b)| a != b)
        .count();
    println!(
        "preámbulo en la muestra {}; {errores} de {} bits llegaron volteados",
        recepcion.posicion,
        enviados.len()
    );
    println!();
    println!("| lectura | enviada | decisión dura | decisión blanda |");
    println!("|---:|---:|---|---:|");
    for (i, ((&enviada, dura), &blanda)) in lecturas
        .iter()
        .zip(&recepcion.cuerpo.duras)
        .zip(&recepcion.cuerpo.blandas)
        .enumerate()
    {
        let dura = match dura {
            Decodificacion::Intacta(x) => format!("{x} (intacta)"),
            Decodificacion::Corregida(x) => format!("{x} (corregida)"),
            Decodificacion::DobleError => "doble error".to_string(),
        };
        println!("| {i} | {enviada} | {dura} | {blanda} |");
    }
}
