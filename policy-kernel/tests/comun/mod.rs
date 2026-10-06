//! Lo que comparten las pruebas: un generador con semilla fija, el evaluador y el validador
//! ingenuos de referencia, y tablas al azar.

// Cada binario de prueba usa una parte distinta de este módulo.
#![allow(dead_code)]

use policy_kernel::orden::{FijarAvance, FijarPresion, MoverValvula, Purgar};
use policy_kernel::politica::{Decision, Motivo, Politica};
use policy_kernel::regla::{Clase, Regla};

/// SplitMix64, con semilla fija.
pub struct Azar(pub u64);

impl Azar {
    pub fn siguiente(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Un entero uniforme en `desde..=hasta`.
    pub fn entre(&mut self, desde: i32, hasta: i32) -> i32 {
        let ancho = (hasta - desde + 1) as u64;
        desde + (self.siguiente() % ancho) as i32
    }
}

/// La decisión, sin la autorización: `Ok` si se permite.
pub type Veredicto = Result<(), Motivo>;

/// Evalúa con la biblioteca una orden de `clase` con el valor `valor`.
pub fn evaluar(politica: &Politica<'_>, clase: Clase, valor: u8) -> Veredicto {
    fn veredicto<O: policy_kernel::orden::Orden>(d: Decision<'_, O>) -> Veredicto {
        match d {
            Decision::Permitir(_) => Ok(()),
            Decision::Denegar(motivo) => Err(motivo),
        }
    }
    match clase {
        Clase::Apertura => veredicto(politica.evaluar(MoverValvula { apertura: valor })),
        Clase::Presion => veredicto(politica.evaluar(FijarPresion { bar: valor })),
        Clase::Avance => veredicto(politica.evaluar(FijarAvance { mm_por_s: valor })),
        Clase::Purga => veredicto(politica.evaluar(Purgar { segundos: valor })),
    }
}

/// El evaluador de referencia, ingenuo a propósito: mira todas las reglas, en cualquier
/// orden, y compara con los límites sin pasar por `Regla::contiene`.
pub fn referencia(reglas: &[Regla], clase: Clase, valor: i32) -> Veredicto {
    let de_la_clase: Vec<&Regla> = reglas.iter().filter(|r| r.clase() == clase).collect();
    if de_la_clase.is_empty() {
        return Err(Motivo::SinRegla);
    }
    if de_la_clase
        .iter()
        .any(|r| r.min() <= valor && valor <= r.max())
    {
        Ok(())
    } else {
        Err(Motivo::FueraDeRango)
    }
}

/// El validador de referencia: ningún rango vacío, ordenada por (clase, mínimo), y ningún par
/// de reglas de la misma clase comparte un valor (comparando todos los pares).
pub fn validar_referencia(reglas: &[Regla]) -> bool {
    let sin_vacios = reglas.iter().all(|r| r.min() <= r.max());
    let ordenada = reglas
        .windows(2)
        .all(|par| (par[0].clase(), par[0].min()) <= (par[1].clase(), par[1].min()));
    let mut sin_solapes = true;
    for (i, a) in reglas.iter().enumerate() {
        for b in &reglas[i + 1..] {
            if a.clase() == b.clase() && a.min().max(b.min()) <= a.max().min(b.max()) {
                sin_solapes = false;
            }
        }
    }
    sin_vacios && ordenada && sin_solapes
}

/// Una tabla válida al azar: para cada clase, de 0 a 4 rangos ordenados y disjuntos entre
/// −20 y 280 (más allá de lo que cabe en un `u8`, de los dos lados).
pub fn tabla_valida(azar: &mut Azar) -> Vec<Regla> {
    let mut reglas = Vec::new();
    for clase in Clase::TODAS {
        let mut desde = -20;
        for _ in 0..azar.entre(0, 4) {
            let min = azar.entre(desde, desde + 60);
            let max = azar.entre(min, min + 70);
            if max > 280 {
                break;
            }
            reglas.push(Regla::nueva(clase, min, max));
            desde = max + 1;
        }
    }
    reglas
}

/// Una tabla cualquiera al azar, válida o no: reglas sueltas, a veces vacías, a veces
/// solapadas o desordenadas.
pub fn tabla_cualquiera(azar: &mut Azar) -> Vec<Regla> {
    (0..azar.entre(0, 6))
        .map(|_| {
            let clase = Clase::TODAS[azar.entre(0, 3) as usize];
            let min = azar.entre(-5, 40);
            let max = azar.entre(min - 3, min + 25);
            Regla::nueva(clase, min, max)
        })
        .collect()
}
