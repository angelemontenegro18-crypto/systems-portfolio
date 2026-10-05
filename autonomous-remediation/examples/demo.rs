//! Del detector a la decisión: dos señales sintéticas, seis propuestas y lo
//! que hace la política con cada una.
//!
//! `cargo run --example demo`. Sin red ni archivos; la salida es determinista.

#[path = "../tests/comun/llegadas.rs"]
mod llegadas;

use autonomous_remediation::detector::{Cusum, Sentido};
use autonomous_remediation::{
    Accion, Actuador, Desenlace, Envolvente, Estado, Metricas, Permiso, Politica, Propuesta,
    Remediador, Servicio, Umbrales,
};

/// Actuador de mentira: anota lo que haría y puede fallar al aplicar.
struct ActuadorDeDemo {
    falla_al_aplicar: bool,
    registro: Vec<&'static str>,
}

impl Actuador for ActuadorDeDemo {
    fn armar_reversion(&mut self, _: &Accion) -> Result<(), String> {
        self.registro.push("armar la vuelta atrás");
        Ok(())
    }
    fn verificar_reversion(&mut self, _: &Accion) -> Result<(), String> {
        self.registro.push("verificarla");
        Ok(())
    }
    fn aplicar(&mut self, _: &Accion) -> Result<(), String> {
        self.registro.push("aplicar");
        if self.falla_al_aplicar {
            Err("el orquestador devolvió un error de cuota".to_string())
        } else {
            Ok(())
        }
    }
    fn comprobar(&mut self, _: &Accion) -> Result<(), String> {
        self.registro.push("comprobar");
        Ok(())
    }
    fn revertir(&mut self, _: &Accion) -> Result<(), String> {
        self.registro.push("revertir");
        Ok(())
    }
}

fn servicio(nombre: &str) -> Servicio {
    Servicio::nuevo(nombre).expect("nombre válido")
}

/// Analiza una señal sintética y devuelve su persistencia, que la demo usa como
/// confianza.
fn senal(nombre: &str, semilla: u64, tramos: &[(usize, f64)]) -> f64 {
    let serie = llegadas::tramos(semilla, tramos);
    let (referencia, resto) = serie.split_at(1000);
    let cusum = Cusum::con_referencia(referencia, 10.0).expect("referencia válida");
    let analisis = cusum.analizar(resto);
    println!("   {nombre}");
    match analisis.alarma {
        Some(alarma) => {
            let hacia = match alarma.sentido {
                Sentido::MasLento => "las respuestas se espaciaron",
                Sentido::MasRapido => "las respuestas se juntaron",
            };
            println!("     alarma en el intervalo {}: {hacia}", alarma.indice);
            println!(
                "     persistencia {:.2} → se usa como confianza\n",
                analisis.persistencia
            );
        }
        None => println!("     sin alarma\n"),
    }
    analisis.persistencia
}

fn main() {
    println!("autonomous-remediation · demo\n");
    println!("1 · Detector: tiempos entre respuestas exitosas de `api`, en milisegundos\n");
    println!("   Referencia: 1000 intervalos con el servicio sano (media 20 ms). Límite de la suma: 10.\n");
    let sostenida = senal(
        "A — desde el intervalo 300, el ritmo cae a un tercio",
        2026,
        &[(1000, 20.0), (300, 20.0), (300, 60.0)],
    );
    let pasajera = senal(
        "B — una ráfaga de 25 intervalos lentos y vuelta a la normalidad",
        2027,
        &[(1000, 20.0), (300, 20.0), (25, 60.0), (275, 20.0)],
    );

    let api = servicio("api");
    let envolvente = Envolvente::vacia()
        .con(Permiso::reiniciar(api.clone()))
        .con(Permiso::vaciar_cache(api.clone()))
        .con(Permiso::escalar(api.clone(), 2, 6).expect("rango válido"))
        .con(Permiso::vaciar_cache(servicio("catalogo")));
    let politica = Politica::nueva(
        envolvente,
        Umbrales::nuevos(0.90, 0.30).expect("umbrales válidos"),
    );
    println!("2 · Política");
    println!("   `api` puede reiniciarse, vaciar su caché y escalar entre 2 y 6 réplicas;");
    println!("   `catalogo` solo puede vaciar su caché. Confianza mínima 0.90; daño colateral máximo 0.30.\n");

    let metricas = |tasa_de_error, utilizacion, aciertos_de_cache, replicas| Metricas {
        tasa_de_error,
        utilizacion,
        aciertos_de_cache,
        replicas,
    };
    let casos = [
        (
            "a",
            false,
            Propuesta {
                accion: Accion::Reiniciar {
                    servicio: api.clone(),
                },
                motivo: "señal A: el ritmo de respuestas exitosas cayó a un tercio".to_string(),
                confianza: Some(sostenida),
                metricas: Some(metricas(0.30, 0.6, 0.5, 4)),
            },
        ),
        (
            "b",
            false,
            Propuesta {
                accion: Accion::Reiniciar {
                    servicio: api.clone(),
                },
                motivo: "señal B: una ráfaga de respuestas lentas".to_string(),
                confianza: Some(pasajera),
                metricas: Some(metricas(0.30, 0.6, 0.5, 4)),
            },
        ),
        (
            "c",
            false,
            Propuesta {
                accion: Accion::VaciarCache {
                    servicio: servicio("catalogo"),
                },
                motivo: "el 20 % de las lecturas devuelve entradas vencidas".to_string(),
                confianza: Some(0.97),
                metricas: Some(metricas(0.20, 0.7, 0.9, 4)),
            },
        ),
        (
            "d",
            false,
            Propuesta {
                accion: Accion::EscalarReplicas {
                    servicio: api.clone(),
                    actuales: 3,
                    nuevas: 8,
                },
                motivo: "utilización 1.4 sostenida".to_string(),
                confianza: Some(0.97),
                metricas: Some(metricas(0.0, 1.4, 0.5, 3)),
            },
        ),
        (
            "e",
            true,
            Propuesta {
                accion: Accion::EscalarReplicas {
                    servicio: api.clone(),
                    actuales: 3,
                    nuevas: 5,
                },
                motivo: "utilización 1.4 sostenida".to_string(),
                confianza: Some(0.97),
                metricas: Some(metricas(0.0, 1.4, 0.5, 3)),
            },
        ),
        (
            "f",
            false,
            Propuesta {
                accion: Accion::EscalarReplicas {
                    servicio: api.clone(),
                    actuales: 3,
                    nuevas: 5,
                },
                motivo: "utilización alta según un tablero que dejó de reportar".to_string(),
                confianza: Some(0.97),
                metricas: None,
            },
        ),
    ];

    let mut ticket_completo = None;
    for (letra, falla_al_aplicar, propuesta) in casos {
        let mut remediador = Remediador::nuevo(
            politica.clone(),
            ActuadorDeDemo {
                falla_al_aplicar,
                registro: Vec::new(),
            },
        );
        let desenlace = remediador.atender(&propuesta);
        println!("   {letra}) {} — {}", propuesta.accion, propuesta.motivo);
        match desenlace {
            Desenlace::Autoaplicada { .. } => println!("      → AUTOAPLICADA"),
            Desenlace::Ticket(ticket) => {
                let resumen = match ticket.estado() {
                    Estado::NoSeAplico => ticket
                        .puertas_fallidas()
                        .into_iter()
                        .map(|p| format!("{p}: {}", ticket.evaluacion().veredicto(p).detalle()))
                        .collect::<Vec<_>>()
                        .join("; "),
                    Estado::Revertida { fallo } => {
                        format!("las cuatro puertas pasaron, pero {fallo}; se revirtió")
                    }
                    Estado::ReversionFallida { fallo, reversion } => {
                        format!("URGENTE: {fallo}, y la vuelta atrás también falló: {reversion}")
                    }
                };
                println!("      → TICKET — {resumen}");
                if letra == "c" {
                    ticket_completo = Some(ticket);
                }
            }
        }
        let registro = &remediador.actuador().registro;
        if registro.is_empty() {
            println!("        actuador: sin tocar\n");
        } else {
            println!("        actuador: {}\n", registro.join(" → "));
        }
    }

    if let Some(ticket) = ticket_completo {
        println!("3 · El ticket de (c), completo\n");
        for linea in ticket.to_string().lines() {
            println!("   {linea}");
        }
    }
}
