//! Un trabajador de demostración: calcula tamaños de miniaturas, una tarea por línea, y
//! termina cuando se cierra su entrada. No abre puertos ni toca la red.

use std::io;

fn main() {
    let entrada = io::stdin().lock();
    let salida = io::stdout().lock();
    if ops_console::trabajador::atender(entrada, salida).is_err() {
        std::process::exit(1);
    }
}
