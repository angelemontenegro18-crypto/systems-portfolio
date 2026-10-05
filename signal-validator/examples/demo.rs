//! Tres demostraciones sobre datos sintéticos:
//!
//! 1. Sobre ruido puro, el k-fold barajado «encuentra» una señal; con purga y
//!    embargo, el acierto vuelve al 50 %.
//! 2. Una señal débil plantada entre veinte candidatas sobrevive al flujo
//!    completo: diseño, Benjamini–Hochberg y confirmación en la reserva sellada.
//! 3. Con cien hipótesis, el umbral fijo descubre lo que no hay; BH no.
//!
//! `cargo run --release --example demo` (en modo debug tarda bastante más).

use signal_validator::azar::Generador;
use signal_validator::fdr::benjamini_hochberg;
use signal_validator::normal::p_bilateral;
use signal_validator::particion::{kfold_barajado, particionar, Intervalo};
use signal_validator::permutacion::{correlacion, prueba_por_bloques};
use signal_validator::sellado::separar;
use signal_validator::sintetico::{acierto_por_vecinos, ruido, senal_plantada, ventanas_pasadas};

fn dos_colas(x: &[f64], y: &[f64]) -> f64 {
    correlacion(x, y).abs()
}

fn main() {
    fuga_sobre_ruido();
    senal_debil();
    muchas_hipotesis();
}

fn fuga_sobre_ruido() {
    println!("1 · Ruido puro: ¿cuánto «acierta» un modelo que no puede acertar?\n");
    println!("   Serie de 3000 valores de ruido. Características: promedios de 20, 50 y 100");
    println!("   valores hacia atrás. Etiqueta: el signo de la suma de los 40 siguientes.");
    println!("   Modelo: 5 vecinos más cercanos. Validación cruzada de 5 pliegues.\n");
    println!("   semilla   k-fold barajado   bloques con purga y embargo");
    let (mut barajados, mut purgados) = (Vec::new(), Vec::new());
    for semilla in 1..=10 {
        let mut g = Generador::nuevo(semilla);
        let serie = ruido(3000, &mut g);
        let conjunto = ventanas_pasadas(&serie, &[20, 50, 100], 40);
        let pliegues_barajados =
            kfold_barajado(conjunto.etiquetas.len(), 5, &mut g).expect("partición válida");
        let pliegues_purgados = particionar(&conjunto.intervalos, 5, 40).expect("partición válida");
        let barajado = acierto_por_vecinos(&conjunto, &pliegues_barajados, 5);
        let purgado = acierto_por_vecinos(&conjunto, &pliegues_purgados, 5);
        println!(
            "   {semilla:>7}   {:>15.1} %   {:>26.1} %",
            100.0 * barajado,
            100.0 * purgado
        );
        barajados.push(barajado);
        purgados.push(purgado);
    }
    let media = |v: &[f64]| 100.0 * v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "   {:>7}   {:>15.1} %   {:>26.1} %",
        "media",
        media(&barajados),
        media(&purgados)
    );
    println!("\n   El acierto verdadero es 50 %: el futuro del ruido no depende de su pasado.\n");
}

fn senal_debil() {
    println!("2 · Una señal débil entre veinte candidatas\n");
    let mut g = Generador::nuevo(2026);
    let escenario = senal_plantada(3000, 20, 7, 0.15, &mut g);
    println!(
        "   20 características candidatas; solo la #{} lleva señal (correlación {:.3} con la etiqueta).",
        escenario.plantada,
        correlacion(&escenario.candidatas[escenario.plantada], &escenario.etiqueta)
    );

    // Una observación por instante: las veinte candidatas y la etiqueta.
    let observaciones: Vec<(Vec<f64>, f64)> = (0..escenario.etiqueta.len())
        .map(|t| {
            (
                escenario.candidatas.iter().map(|c| c[t]).collect(),
                escenario.etiqueta[t],
            )
        })
        .collect();
    let intervalos: Vec<Intervalo> = (0..observaciones.len() as u64)
        .map(|t| Intervalo::nuevo(t, t).expect("válido"))
        .collect();
    let separacion = separar(observaciones, &intervalos, 2000).expect("separación válida");
    println!(
        "   Reserva: las últimas {} observaciones, selladas antes de mirar nada.",
        separacion.reserva.len()
    );
    println!(
        "   Compromiso SHA-256: {}\n",
        separacion.reserva.compromiso()
    );

    let etiqueta: Vec<f64> = separacion.diseno.iter().map(|(_, y)| *y).collect();
    let p: Vec<f64> = (0..20)
        .map(|j| {
            let x: Vec<f64> = separacion.diseno.iter().map(|(c, _)| c[j]).collect();
            prueba_por_bloques(&x, &etiqueta, 50, 999, &mut g, dos_colas)
                .expect("prueba válida")
                .p_valor
        })
        .collect();
    let elegidas = benjamini_hochberg(&p, 0.10).expect("p-valores válidos");
    let mut orden: Vec<usize> = (0..20).collect();
    orden.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    println!("   Diseño (2000 observaciones), permutación por bloques de 50, 999 permutaciones:");
    for &j in &orden[..5] {
        let marca = if elegidas[j] {
            "← elegida por BH (q = 0.10)"
        } else {
            ""
        };
        println!(
            "{}",
            format!("     candidata #{j:<2}  p = {:.3}  {marca}", p[j]).trim_end()
        );
    }
    println!("     … y otras 15 con p mayor.\n");

    let reserva = separacion.reserva.abrir();
    let etiqueta: Vec<f64> = reserva.iter().map(|(_, y)| *y).collect();
    println!("   La reserva se abre una sola vez, solo para las elegidas:");
    for j in (0..20).filter(|&j| elegidas[j]) {
        let x: Vec<f64> = reserva.iter().map(|(c, _)| c[j]).collect();
        let prueba =
            prueba_por_bloques(&x, &etiqueta, 50, 999, &mut g, dos_colas).expect("prueba válida");
        let veredicto = if prueba.p_valor <= 0.05 {
            "confirmada"
        } else {
            "no se confirma"
        };
        println!(
            "     candidata #{j:<2}  p = {:.3}  → {veredicto}",
            prueba.p_valor
        );
    }
    println!();
}

fn muchas_hipotesis() {
    println!("3 · Cien hipótesis a la vez, 1000 repeticiones\n");
    println!("   Cada hipótesis es una prueba z sobre la media de 20 valores normales.\n");
    let mut g = Generador::nuevo(2026);
    for (titulo, con_efecto) in [
        ("Las 100 nulas", 0),
        ("90 nulas y 10 con efecto real (media 0.8)", 10),
    ] {
        let (mut falsos_fijo, mut algun_falso_bh) = (0.0, 0usize);
        let (mut fdp_fijo, mut fdp_bh, mut potencia_fijo, mut potencia_bh) = (0.0, 0.0, 0.0, 0.0);
        let repeticiones = 1000;
        for _ in 0..repeticiones {
            let p: Vec<f64> = (0..100)
                .map(|h| {
                    let media = if h < con_efecto { 0.8 } else { 0.0 };
                    let suma: f64 = (0..20).map(|_| media + g.normal()).sum();
                    p_bilateral(suma / 20f64.sqrt())
                })
                .collect();
            let fijo: Vec<bool> = p.iter().map(|&v| v <= 0.05).collect();
            let bh = benjamini_hochberg(&p, 0.05).expect("p-valores válidos");
            let falsos = |r: &[bool]| r[con_efecto..].iter().filter(|&&x| x).count();
            let verdaderos = |r: &[bool]| r[..con_efecto].iter().filter(|&&x| x).count();
            let proporcion = |r: &[bool]| {
                let declarados = falsos(r) + verdaderos(r);
                if declarados == 0 {
                    0.0
                } else {
                    falsos(r) as f64 / declarados as f64
                }
            };
            falsos_fijo += falsos(&fijo) as f64;
            algun_falso_bh += usize::from(falsos(&bh) > 0);
            fdp_fijo += proporcion(&fijo);
            fdp_bh += proporcion(&bh);
            if con_efecto > 0 {
                potencia_fijo += verdaderos(&fijo) as f64 / con_efecto as f64;
                potencia_bh += verdaderos(&bh) as f64 / con_efecto as f64;
            }
        }
        let r = repeticiones as f64;
        println!("   {titulo}:");
        if con_efecto == 0 {
            println!(
                "     umbral fijo 0.05: {:.2} descubrimientos falsos por repetición",
                falsos_fijo / r
            );
            println!(
                "     BH, q = 0.05:     algún descubrimiento falso en el {:.1} % de las repeticiones",
                100.0 * algun_falso_bh as f64 / r
            );
        } else {
            println!(
                "     umbral fijo 0.05: {:.1} % de lo declarado es falso; encuentra el {:.1} % de los efectos",
                100.0 * fdp_fijo / r,
                100.0 * potencia_fijo / r
            );
            println!(
                "     BH, q = 0.05:     {:.1} % de lo declarado es falso; encuentra el {:.1} % de los efectos",
                100.0 * fdp_bh / r,
                100.0 * potencia_bh / r
            );
        }
        println!();
    }
}
