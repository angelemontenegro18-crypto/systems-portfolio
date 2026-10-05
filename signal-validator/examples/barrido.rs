//! Los escenarios de la demo, repetidos en muchas semillas: para que ninguna
//! cifra del README dependa de una semilla con suerte.
//!
//! `cargo run --release --example barrido` (unos segundos en release).

use signal_validator::azar::Generador;
use signal_validator::fdr::benjamini_hochberg;
use signal_validator::particion::{kfold_barajado, particionar};
use signal_validator::permutacion::{correlacion, prueba_por_bloques};
use signal_validator::sintetico::{
    acierto_por_vecinos, ar1, ruido, senal_plantada, ventanas_pasadas,
};

fn dos_colas(x: &[f64], y: &[f64]) -> f64 {
    correlacion(x, y).abs()
}

fn resumen(nombre: &str, valores: &[f64]) {
    let media = valores.iter().sum::<f64>() / valores.len() as f64;
    let minimo = valores.iter().copied().fold(f64::INFINITY, f64::min);
    let maximo = valores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    println!(
        "   {nombre:<30} media {:.1} %   mínimo {:.1} %   máximo {:.1} %",
        100.0 * media,
        100.0 * minimo,
        100.0 * maximo
    );
}

fn main() {
    println!("1 · Ruido puro, semillas 1 a 30 (mismo escenario que la demo)\n");
    let (mut barajados, mut purgados) = (Vec::new(), Vec::new());
    for semilla in 1..=30 {
        let mut g = Generador::nuevo(semilla);
        let serie = ruido(3000, &mut g);
        let conjunto = ventanas_pasadas(&serie, &[20, 50, 100], 40);
        let b = kfold_barajado(conjunto.etiquetas.len(), 5, &mut g).expect("válido");
        let p = particionar(&conjunto.intervalos, 5, 40).expect("válido");
        barajados.push(acierto_por_vecinos(&conjunto, &b, 5));
        purgados.push(acierto_por_vecinos(&conjunto, &p, 5));
    }
    resumen("k-fold barajado", &barajados);
    resumen("bloques con purga y embargo", &purgados);

    println!("\n2 · Señal débil entre veinte candidatas, semillas 1 a 40\n");
    let semillas = 40;
    let (mut detectada, mut confirmada, mut falsas_en_diseno, mut falsas_confirmadas) =
        (0, 0, 0, 0);
    for semilla in 1..=semillas {
        let mut g = Generador::nuevo(semilla);
        let escenario = senal_plantada(3000, 20, 7, 0.15, &mut g);
        let (diseno, reserva) = (0..2000, 2000..3000);
        let y = &escenario.etiqueta;
        let p: Vec<f64> = escenario
            .candidatas
            .iter()
            .map(|c| {
                prueba_por_bloques(
                    &c[diseno.clone()],
                    &y[diseno.clone()],
                    50,
                    999,
                    &mut g,
                    dos_colas,
                )
                .expect("válida")
                .p_valor
            })
            .collect();
        let elegidas = benjamini_hochberg(&p, 0.10).expect("válidos");
        for (j, _) in elegidas.iter().enumerate().filter(|(_, &e)| e) {
            let c = &escenario.candidatas[j];
            let confirma = prueba_por_bloques(
                &c[reserva.clone()],
                &y[reserva.clone()],
                50,
                999,
                &mut g,
                dos_colas,
            )
            .expect("válida")
            .p_valor
                <= 0.05;
            if j == escenario.plantada {
                detectada += 1;
                confirmada += usize::from(confirma);
            } else {
                falsas_en_diseno += 1;
                falsas_confirmadas += usize::from(confirma);
            }
        }
    }
    println!("   la señal, elegida por BH en el diseño:   {detectada} de {semillas}");
    println!("   la señal, confirmada en la reserva:      {confirmada} de {semillas}");
    println!("   candidatas sin señal elegidas en diseño: {falsas_en_diseno} en total");
    println!("   … y confirmadas después en la reserva:   {falsas_confirmadas}");

    println!("\n3 · Dos series independientes con memoria (AR(1), φ = 0.8), 300 pares\n");
    let mut g = Generador::nuevo(99);
    let (mut de_a_uno, mut por_bloques) = (0, 0);
    for _ in 0..300 {
        let x = ar1(500, 0.8, &mut g);
        let y = ar1(500, 0.8, &mut g);
        de_a_uno += usize::from(
            prueba_por_bloques(&x, &y, 1, 199, &mut g, dos_colas)
                .expect("válida")
                .p_valor
                <= 0.05,
        );
        por_bloques += usize::from(
            prueba_por_bloques(&x, &y, 25, 199, &mut g, dos_colas)
                .expect("válida")
                .p_valor
                <= 0.05,
        );
    }
    println!(
        "   rechazos al 5 %, permutando de a uno:      {:.1} %",
        100.0 * de_a_uno as f64 / 300.0
    );
    println!(
        "   rechazos al 5 %, permutando bloques de 25: {:.1} %",
        100.0 * por_bloques as f64 / 300.0
    );

    println!("\n4 · La prueba de confirmación de la sección 2, bajo la hipótesis nula\n");
    // Misma forma que la confirmación en la reserva: una candidata sin señal
    // contra la etiqueta del escenario, 1000 observaciones, bloques de 50.
    let mut g = Generador::nuevo(12345);
    let (mut rechazos, mut pruebas) = (0, 0);
    for _ in 0..600 {
        let escenario = senal_plantada(1000, 3, 0, 0.15, &mut g);
        for sin_senal in &escenario.candidatas[1..] {
            let p = prueba_por_bloques(sin_senal, &escenario.etiqueta, 50, 199, &mut g, dos_colas)
                .expect("válida")
                .p_valor;
            rechazos += usize::from(p <= 0.05);
            pruebas += 1;
        }
    }
    println!(
        "   rechazos al 5 % en {pruebas} pruebas sin señal: {:.1} %",
        100.0 * rechazos as f64 / pruebas as f64
    );
}
