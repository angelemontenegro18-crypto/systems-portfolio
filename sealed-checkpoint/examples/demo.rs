//! Recorrido: guardar, cargar, y tres ataques que el almacén detecta.
//!
//! ```text
//! cargo run --example demo
//! ```

use std::fs;

use sealed_checkpoint::{sello, Almacen, Clave};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("sealed-checkpoint-demo-{}", std::process::id()));
    let almacen = Almacen::abrir(&dir, Clave::generar()?)?;
    let ruta = almacen.ruta_de("sesion")?;

    println!("1 · Guardar y cargar\n");
    almacen.guardar("sesion", 1, br#"{"usuarios_activos": 12}"#)?;
    let copia_gen_1 = fs::read(&ruta)?;
    almacen.guardar("sesion", 2, br#"{"usuarios_activos": 15}"#)?;
    if let Some(c) = almacen.cargar("sesion")? {
        println!(
            "   generación {} · {}",
            c.generacion,
            String::from_utf8_lossy(&c.datos)
        );
    }
    let bytes = fs::read(&ruta)?;
    println!(
        "   en disco: {} bytes, generación legible sin la clave: {}",
        bytes.len(),
        sello::generacion_sin_verificar(&bytes)?
    );

    println!("\n2 · Un byte alterado en disco\n");
    let mut alterado = bytes.clone();
    alterado[40] ^= 1;
    fs::write(&ruta, &alterado)?;
    println!("   cargar → {}", resultado(almacen.cargar("sesion")));
    fs::write(&ruta, &bytes)?;

    println!("\n3 · El archivo copiado con otro nombre\n");
    fs::copy(&ruta, almacen.ruta_de("config")?)?;
    println!(
        "   cargar(\"config\") → {}",
        resultado(almacen.cargar("config"))
    );

    println!("\n4 · Alguien repone una copia vieja (auténtica)\n");
    fs::write(&ruta, &copia_gen_1)?;
    println!(
        "   cargar              → {}",
        resultado(almacen.cargar("sesion"))
    );
    println!(
        "   cargar_desde(mín 2) → {}",
        resultado(almacen.cargar_desde("sesion", 2))
    );

    println!("\n5 · Guardar una generación que no avanza\n");
    fs::write(&ruta, &bytes)?;
    match almacen.guardar("sesion", 2, b"otro") {
        Ok(()) => println!("   (se guardó: no debería)"),
        Err(e) => println!("   rechazado: {e}"),
    }

    fs::remove_dir_all(&dir)?;
    Ok(())
}

fn resultado<E: std::fmt::Display>(r: Result<Option<sealed_checkpoint::Abierto>, E>) -> String {
    match r {
        Ok(Some(a)) => format!("ok, generación {}", a.generacion),
        Ok(None) => "no hay checkpoint".to_string(),
        Err(e) => format!("error: {e}"),
    }
}
