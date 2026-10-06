//! Mide la ganancia de procesamiento: cuánto más fuerte puede ser un tono interferente, con
//! cada largo de código, antes de que la tasa de error de bit llegue a 10⁻³.
//!
//! ```text
//! cargo run --release --example ganancia
//! ```
//!
//! Para cada `N` busca, por búsqueda binaria, la amplitud entera de tono más alta con menos
//! de un error cada mil bits, y la compara con la de un bit sin esparcir (`N = 1`) a la misma
//! amplitud de chip y con el mismo ruido. La ganancia medida es `20 · log₁₀` del cociente de
//! amplitudes, y la teórica, `10 · log₁₀(N)`. La columna «sin ruido» repite la medición con
//! el ruido apagado, para separar lo que aporta el tono de lo que aporta el ruido.

#[path = "../tests/canal/mod.rs"]
mod canal;

use canal::{ganancia_db, tono_tolerado};
use spread_spectrum_codec::lfsr::{Codigo, Polinomio};

const AMPLITUD: i32 = 1000;
const SIGMA: i64 = 100;
const BITS: usize = 200_000;
const SEMILLA: u64 = 2026;

fn main() {
    println!(
        "amplitud de chip {AMPLITUD}, ruido σ = {SIGMA} por chip, tono de periodo 16 chips, \
         {BITS} bits por intento"
    );
    println!(
        "semillas: {SEMILLA} para los bits y {} para el ruido",
        SEMILLA + 1
    );
    println!();

    let referencia = tono_tolerado(None, AMPLITUD, SIGMA, BITS, SEMILLA);
    let referencia_sin_ruido = tono_tolerado(None, AMPLITUD, 0, BITS, SEMILLA);
    println!(
        "| N | secuencia | tono tolerado | ganancia medida | sin ruido: tono | sin ruido: ganancia | 10·log₁₀(N) |"
    );
    println!("|---:|---|---:|---:|---:|---:|---:|");
    println!("| 1 | (sin esparcir) | {referencia} | — | {referencia_sin_ruido} | — | — |");
    for p in Polinomio::POR_GRADO {
        let codigo = Codigo::secuencia_m(p);
        let n = codigo.largo();
        let tolerado = tono_tolerado(Some(&codigo), AMPLITUD, SIGMA, BITS, SEMILLA);
        let sin_ruido = tono_tolerado(Some(&codigo), AMPLITUD, 0, BITS, SEMILLA);
        println!(
            "| {n} | grado {} | {tolerado} | {:.2} dB | {sin_ruido} | {:.2} dB | {:.2} dB |",
            p.grado(),
            ganancia_db(tolerado, referencia),
            ganancia_db(sin_ruido, referencia_sin_ruido),
            10.0 * (n as f64).log10()
        );
    }
}
