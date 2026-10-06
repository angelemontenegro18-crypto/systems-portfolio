//! El canal simulado de las pruebas y de los ejemplos: ruido gaussiano aproximado, un
//! interferente de banda estrecha y ráfagas, todo con enteros y con un generador
//! pseudoaleatorio propio de semilla fija. También arma y lee la trama de las pruebas
//! (preámbulo + lecturas protegidas y entrelazadas).
//!
//! Vive en `tests/` porque usa `std` y no es parte de la biblioteca; los ejemplos lo incluyen
//! con `#[path]`.

// Cada binario de prueba usa una parte distinta de este módulo.
#![allow(dead_code)]

use std::ops::Range;

use spread_spectrum_codec::ensanchado::{correlar, decidir, esparcir, recortar};
use spread_spectrum_codec::entrelazado::{desentrelazar, desentrelazar_blandos, entrelazar};
use spread_spectrum_codec::hamming::{codificar, decodificar, decodificar_blando, Decodificacion};
use spread_spectrum_codec::lfsr::Codigo;
use spread_spectrum_codec::sincronia::{buscar_preambulo, umbral_de_media_altura};

/// SplitMix64: 64 bits de estado, rápido y reproducible. Sirve para simular un canal; no es
/// criptográfico.
pub struct Azar {
    estado: u64,
}

impl Azar {
    pub fn nuevo(semilla: u64) -> Azar {
        Azar { estado: semilla }
    }

    pub fn siguiente(&mut self) -> u64 {
        self.estado = self.estado.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.estado;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Un bit, 0 o 1.
    pub fn bit(&mut self) -> u8 {
        (self.siguiente() >> 63) as u8
    }

    /// Un entero uniforme en `0..n`, con `n > 0`.
    pub fn hasta(&mut self, n: u64) -> u64 {
        ((u128::from(self.siguiente()) * u128::from(n)) >> 64) as u64
    }

    /// Ruido de media cero y desviación `sigma`: la suma de 12 uniformes enteras en
    /// `[−2048, 2047]` (desviación 4096), centrada y escalada. Es una aproximación: no tiene
    /// colas más allá de seis desviaciones.
    pub fn gaussiano(&mut self, sigma: i64) -> i64 {
        let mut suma = 0i64;
        for _ in 0..3 {
            let x = self.siguiente();
            for k in 0..4 {
                suma += ((x >> (12 * k)) & 0xFFF) as i64 - 2048;
            }
        }
        // Cada uniforme tiene media −1/2: las doce suman −6 en promedio.
        (suma + 6) * sigma / 4096
    }
}

/// Un periodo de un seno de 16 muestras, en milésimas: el interferente de banda estrecha es
/// un tono a 1/16 de la frecuencia de chip.
pub const SENO_16: [i64; 16] = [
    0, 383, 707, 924, 1000, 924, 707, 383, 0, -383, -707, -924, -1000, -924, -707, -383,
];

/// Un canal aditivo: ruido de desviación `sigma` y un tono de amplitud `tono`. El tono sigue
/// en fase de una llamada a la siguiente.
pub struct Canal {
    azar: Azar,
    sigma: i64,
    tono: i64,
    reloj: usize,
}

impl Canal {
    pub fn nuevo(semilla: u64, sigma: i64, tono: i64) -> Canal {
        Canal {
            azar: Azar::nuevo(semilla),
            sigma,
            tono,
            reloj: 0,
        }
    }

    pub fn pasar(&mut self, muestras: &mut [i32]) {
        for m in muestras.iter_mut() {
            let interferente = self.tono * SENO_16[self.reloj % 16] / 1000;
            let ruido = if self.sigma == 0 {
                0
            } else {
                self.azar.gaussiano(self.sigma)
            };
            *m = saturar(i64::from(*m) + interferente + ruido);
            self.reloj += 1;
        }
    }
}

fn saturar(x: i64) -> i32 {
    x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Una ráfaga que invierte la fase de las muestras de `rango`: cada bit que cubre entero sale
/// con el signo contrario. Es el peor caso para el entrelazador.
pub fn rafaga_invertida(muestras: &mut [i32], rango: Range<usize>) {
    for m in &mut muestras[rango] {
        *m = m.saturating_neg();
    }
}

/// Una ráfaga de ruido fuerte, de desviación `sigma`, sobre las muestras de `rango`.
pub fn rafaga_de_ruido(muestras: &mut [i32], rango: Range<usize>, sigma: i64, azar: &mut Azar) {
    for m in &mut muestras[rango] {
        *m = saturar(i64::from(*m) + azar.gaussiano(sigma));
    }
}

// ---------------------------------------------------------------------------------------------
// La medición de la ganancia de procesamiento.

/// Cuántos bits se procesan por tanda, para no reservar memoria de más.
const TANDA: usize = 4096;

/// Cuántos de `bits` bits aleatorios llegan mal al cruzar un canal con ruido `sigma` y un tono
/// de amplitud `tono`. Con `codigo = None` no se esparce: cada bit es una sola muestra de
/// amplitud `amplitud`, y se decide por el signo, como haría la correlación con un chip.
///
/// Los bits salen de `semilla` y el ruido de `semilla + 1`: con la misma semilla, dos tonos
/// distintos ven los mismos bits y el mismo ruido.
pub fn errores_de_bit(
    codigo: Option<&Codigo>,
    amplitud: i32,
    sigma: i64,
    tono: i64,
    bits: usize,
    semilla: u64,
) -> usize {
    let n = codigo.map_or(1, Codigo::largo);
    let mut datos = Azar::nuevo(semilla);
    let mut canal = Canal::nuevo(semilla.wrapping_add(1), sigma, tono);
    let mut enviados = vec![0u8; TANDA];
    let mut recibidos = vec![0u8; TANDA];
    let mut muestras = vec![0i32; TANDA * n];
    let mut errores = 0;
    let mut faltan = bits;
    while faltan > 0 {
        let k = faltan.min(TANDA);
        let (enviados, recibidos, muestras) = (
            &mut enviados[..k],
            &mut recibidos[..k],
            &mut muestras[..k * n],
        );
        for b in enviados.iter_mut() {
            *b = datos.bit();
        }
        match codigo {
            Some(c) => {
                esparcir(enviados, c, amplitud, muestras).expect("búferes a medida");
            }
            None => {
                for (m, &b) in muestras.iter_mut().zip(enviados.iter()) {
                    *m = if b == 0 { amplitud } else { -amplitud };
                }
            }
        }
        canal.pasar(muestras);
        match codigo {
            Some(c) => {
                decidir(muestras, c, recibidos).expect("búferes a medida");
            }
            None => {
                for (r, &m) in recibidos.iter_mut().zip(muestras.iter()) {
                    *r = if m >= 0 { 0 } else { 1 };
                }
            }
        }
        errores += enviados
            .iter()
            .zip(recibidos.iter())
            .filter(|(a, b)| a != b)
            .count();
        faltan -= k;
    }
    errores
}

/// La amplitud de tono más alta que deja la tasa de error de bit por debajo de 10⁻³: búsqueda
/// binaria sobre la amplitud entera, con la misma semilla en cada intento.
pub fn tono_tolerado(
    codigo: Option<&Codigo>,
    amplitud: i32,
    sigma: i64,
    bits: usize,
    semilla: u64,
) -> i64 {
    let tolera =
        |tono: i64| errores_de_bit(codigo, amplitud, sigma, tono, bits, semilla) * 1000 < bits;
    let n = codigo.map_or(1, Codigo::largo) as i64;
    let (mut bajo, mut alto) = (0, 4 * i64::from(amplitud) * n);
    assert!(tolera(bajo), "sin tono, el ruido ya pasa de 10⁻³");
    assert!(
        !tolera(alto),
        "el tope de la búsqueda no alcanza a molestar"
    );
    while alto - bajo > 1 {
        let medio = bajo + (alto - bajo) / 2;
        if tolera(medio) {
            bajo = medio;
        } else {
            alto = medio;
        }
    }
    bajo
}

/// `20 · log₁₀(tolerado / referencia)`: la ganancia en potencia del tono tolerado.
pub fn ganancia_db(tolerado: i64, referencia: i64) -> f64 {
    20.0 * (tolerado as f64 / referencia as f64).log10()
}

// ---------------------------------------------------------------------------------------------
// La trama de las pruebas: preámbulo, y después las lecturas en Hamming (8,4), entrelazadas y
// esparcidas.

/// Los bits del cuerpo de la trama: las `lecturas` (de 0 a 15) en Hamming (8,4), entrelazadas.
pub fn bits_de_las_lecturas(lecturas: &[u8]) -> Vec<u8> {
    let palabras: Vec<u8> = lecturas
        .iter()
        .map(|&l| codificar(l).expect("lectura de 4 bits"))
        .collect();
    let mut bits = vec![0u8; 8 * palabras.len()];
    entrelazar(&palabras, &mut bits).expect("búfer a medida");
    bits
}

/// Arma la trama: el preámbulo (como bits 0) y las `lecturas` (de 0 a 15).
pub fn armar_trama(
    lecturas: &[u8],
    codigo: &Codigo,
    preambulo: &Codigo,
    amplitud: i32,
) -> Vec<i32> {
    let bits = bits_de_las_lecturas(lecturas);
    let mut trama = vec![0i32; preambulo.largo() + bits.len() * codigo.largo()];
    let (inicio, datos) = trama.split_at_mut(preambulo.largo());
    esparcir(&[0], preambulo, amplitud, inicio).expect("búfer a medida");
    esparcir(&bits, codigo, amplitud, datos).expect("búfer a medida");
    trama
}

/// Lo que el receptor sacó del cuerpo de una trama.
#[derive(Debug)]
pub struct Cuerpo {
    /// Los bits con decisión dura, antes de desentrelazar.
    pub bits: Vec<u8>,
    /// Cada lectura, con decisión dura.
    pub duras: Vec<Decodificacion>,
    /// Cada lectura, con decisión blanda.
    pub blandas: Vec<u8>,
}

/// Lo que el receptor sacó de una trama: dónde estaba, y su cuerpo.
#[derive(Debug)]
pub struct Recepcion {
    /// Dónde encontró el preámbulo.
    pub posicion: usize,
    pub cuerpo: Cuerpo,
}

/// Lee `cuantas` lecturas de `datos`, el cuerpo de una trama ya sincronizada, con las dos
/// decisiones. Con `recorte`, los valores blandos se recortan a `N · recorte` (la amplitud
/// esperada) antes de decodificar.
pub fn leer_cuerpo(datos: &[i32], cuantas: usize, codigo: &Codigo, recorte: Option<i32>) -> Cuerpo {
    let mut bits = vec![0u8; 8 * cuantas];
    decidir(datos, codigo, &mut bits).expect("búfer a medida");
    let mut palabras = vec![0u8; cuantas];
    desentrelazar(&bits, &mut palabras).expect("búfer a medida");
    let duras = palabras.iter().map(|&p| decodificar(p)).collect();

    let mut blandos = vec![0i64; 8 * cuantas];
    correlar(datos, codigo, &mut blandos).expect("búfer a medida");
    if let Some(amplitud) = recorte {
        recortar(&mut blandos, codigo.largo() as i64 * i64::from(amplitud));
    }
    let mut por_palabra = vec![[0i64; 8]; cuantas];
    desentrelazar_blandos(&blandos, &mut por_palabra).expect("búfer a medida");
    let blandas = por_palabra.iter().map(decodificar_blando).collect();

    Cuerpo {
        bits,
        duras,
        blandas,
    }
}

/// Busca el preámbulo con el umbral de media altura y lee `cuantas` lecturas detrás de él,
/// con los valores blandos recortados a `N · amplitud`. `None` si no hay preámbulo o si la
/// trama está cortada.
pub fn leer_trama(
    muestras: &[i32],
    cuantas: usize,
    codigo: &Codigo,
    preambulo: &Codigo,
    amplitud: i32,
) -> Option<Recepcion> {
    let umbral = umbral_de_media_altura(preambulo, amplitud);
    let pico = buscar_preambulo(muestras, preambulo, umbral)?;
    let inicio = pico.posicion + preambulo.largo();
    let datos = muestras.get(inicio..inicio + 8 * cuantas * codigo.largo())?;
    Some(Recepcion {
        posicion: pico.posicion,
        cuerpo: leer_cuerpo(datos, cuantas, codigo, Some(amplitud)),
    })
}
