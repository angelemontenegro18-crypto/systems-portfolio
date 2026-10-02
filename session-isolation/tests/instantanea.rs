//! El seqlock bajo carga: un escritor publicando sin parar y varios lectores
//! leyendo sin parar. Ninguna lectura puede ver una mezcla de dos escrituras.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use session_isolation::{Codificable, Publicador};

/// Cuatro palabras que siempre deben ser iguales entre sí. Una lectura rota
/// (mitad de una escritura, mitad de otra) lo delataría de inmediato.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cuadruple([u64; 4]);

impl Codificable<4> for Cuadruple {
    fn a_palabras(&self) -> [u64; 4] {
        self.0
    }
    fn desde_palabras(p: [u64; 4]) -> Self {
        Cuadruple(p)
    }
}

#[test]
fn ninguna_lectura_mezcla_dos_escrituras() {
    const ESCRITURAS: u64 = 200_000;
    const LECTORES: usize = 4;

    let publicador = Arc::new(Publicador::nuevo(&Cuadruple([0; 4])));
    let terminado = Arc::new(AtomicBool::new(false));

    let lectores: Vec<_> = (0..LECTORES)
        .map(|_| {
            let p = Arc::clone(&publicador);
            let fin = Arc::clone(&terminado);
            thread::spawn(move || {
                let mut lecturas = 0u64;
                let mut ultima = 0u64;
                while !fin.load(Ordering::Relaxed) {
                    let i = p.leer();
                    let [a, b, c, d] = i.valor.0;
                    assert!(a == b && b == c && c == d, "lectura rota: {:?}", i.valor);
                    // Cada escritura publica su número de generación en las 4 palabras.
                    assert_eq!(a, i.generacion, "el valor no corresponde a su generación");
                    assert!(i.generacion >= ultima, "la generación retrocedió");
                    ultima = i.generacion;
                    lecturas += 1;
                }
                lecturas
            })
        })
        .collect();

    for g in 1..=ESCRITURAS {
        assert_eq!(publicador.publicar(&Cuadruple([g; 4])), g);
    }
    terminado.store(true, Ordering::Relaxed);

    let total: u64 = lectores.into_iter().map(|h| h.join().expect("lector sin pánico")).sum();
    assert!(total > 0, "los lectores tienen que haber leído");
    assert_eq!(publicador.leer().valor, Cuadruple([ESCRITURAS; 4]));
}

#[test]
fn varios_escritores_no_rompen_la_secuencia() {
    let p = Arc::new(Publicador::nuevo(&Cuadruple([0; 4])));
    let escritores: Vec<_> = (0..4)
        .map(|_| {
            let p = Arc::clone(&p);
            thread::spawn(move || {
                for _ in 0..10_000 {
                    // El valor no importa: importa que sea consistente.
                    p.publicar(&Cuadruple([7; 4]));
                    let [a, b, c, d] = p.leer().valor.0;
                    assert!(a == b && b == c && c == d);
                }
            })
        })
        .collect();
    for e in escritores {
        e.join().expect("escritor sin pánico");
    }
    assert_eq!(p.generacion(), 40_000, "cada publicación contó exactamente una vez");
}
