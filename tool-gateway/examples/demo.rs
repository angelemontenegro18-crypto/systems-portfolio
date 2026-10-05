//! Simula el lado del modelo: recibe el manifiesto y hace llamadas, algunas
//! bien formadas y otras no, como pasa en la práctica.
//!
//! ```text
//! cargo run --example demo
//! ```

use serde_json::{json, Value};
use tool_gateway::{catalogo, Herramienta, Registro};

fn nada(_: &Value) -> Result<Value, String> {
    Ok(json!({}))
}

fn main() {
    let registro = match catalogo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("catálogo inválido: {e}");
            return;
        }
    };

    println!(
        "Manifiesto que recibe el modelo ({} herramientas):",
        registro.len()
    );
    for h in registro.herramientas() {
        println!("  · {:<22} {}", h.nombre, h.descripcion);
    }

    println!("\nLlamadas del modelo:");
    let llamadas = [
        (
            "distancia",
            json!({"a": {"x": 1, "y": 1}, "b": {"x": 4, "y": 5}}),
        ),
        ("estadisticas", json!({"valores": [3, "cuatro", 5]})),
        (
            "convertir_temperatura",
            json!({"valor": 98.6, "de": "fahrenheit", "a": "celsius"}),
        ),
        ("validar_isbn", json!({"isbn": "978-84-376-0494-8"})),
        ("leer_archivo", json!({"ruta": "/etc/passwd"})),
    ];
    for (i, (nombre, args)) in llamadas.iter().enumerate() {
        let r = registro.responder(&format!("llamada_{i}"), nombre, args);
        let marca = if r["is_error"] == true {
            "error"
        } else {
            "ok   "
        };
        println!(
            "  [{marca}] {nombre:<22} → {}",
            r["content"].as_str().unwrap_or_default()
        );
    }

    println!("\nIntento de registrar una herramienta vetada:");
    let mut r = Registro::nuevo();
    let vetada = Herramienta {
        nombre: "ejecutar_comando",
        descripcion: "ejecuta lo que el modelo pida",
        esquema: json!({"type": "object"}),
        ejemplo: json!({}),
        funcion: nada,
    };
    match r.registrar(vetada) {
        Ok(()) => println!("  (se registró: esto no debería pasar)"),
        Err(e) => println!("  rechazada: {e}"),
    }
}
