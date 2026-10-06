//! El panel de la prensa manda órdenes; solo las que la política autoriza llegan al
//! hardware, y solo con el interbloqueo armado.
//!
//! ```text
//! cargo run --example prensa
//! ```

use policy_kernel::interbloqueo::{Armado, Interbloqueo};
use policy_kernel::orden::{FijarAvance, FijarPresion, MoverValvula, Orden, Purgar};
use policy_kernel::politica::{Decision, PRENSA};

/// Evalúa una orden y, si se permite, la aplica. Cuenta qué pasó.
fn panel<O: Orden + core::fmt::Debug>(interbloqueo: &mut Interbloqueo<Armado>, orden: O) {
    let descripcion = format!("{orden:?}");
    match PRENSA.evaluar(orden) {
        Decision::Permitir(autorizada) => {
            let comando = interbloqueo.aplicar(autorizada);
            println!(
                "{descripcion:<32} → al hardware: {:?} = {}",
                comando.clase(),
                comando.valor()
            );
        }
        Decision::Denegar(motivo) => println!("{descripcion:<32} → denegada ({motivo:?})"),
    }
}

fn main() {
    let desarmado = Interbloqueo::nuevo();
    // El operador intenta armar con el resguardo abierto: sigue desarmado.
    let desarmado = match desarmado.armar(false) {
        Ok(_) => unreachable!("con el resguardo abierto no se arma"),
        Err(sigue) => {
            println!("resguardo abierto: el interbloqueo sigue desarmado");
            sigue
        }
    };
    let Ok(mut armado) = desarmado.armar(true) else {
        unreachable!("con el resguardo cerrado se arma")
    };
    println!("resguardo cerrado: armado\n");

    panel(&mut armado, MoverValvula { apertura: 30 });
    panel(&mut armado, MoverValvula { apertura: 85 });
    panel(&mut armado, MoverValvula { apertura: 95 });
    panel(&mut armado, MoverValvula { apertura: 120 });
    panel(&mut armado, FijarPresion { bar: 150 });
    panel(&mut armado, FijarPresion { bar: 200 });
    panel(&mut armado, FijarAvance { mm_por_s: 25 });
    panel(&mut armado, FijarAvance { mm_por_s: 26 });
    panel(&mut armado, Purgar { segundos: 5 });

    let desarmado = armado.desarmar();
    println!("\naplicadas: {}; desarmado", desarmado.aplicadas());
}
