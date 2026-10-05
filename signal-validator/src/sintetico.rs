//! Datos sintéticos y un clasificador mínimo, para demostrar las garantías.
//!
//! Nada de esto es parte del núcleo: son las piezas con las que la demo y los
//! tests arman series donde se sabe de antemano qué señal hay, o que no hay
//! ninguna.

use crate::azar::Generador;
use crate::particion::{Intervalo, Pliegue};

/// `n` valores de ruido normal independiente.
pub fn ruido(n: usize, generador: &mut Generador) -> Vec<f64> {
    (0..n).map(|_| generador.normal()).collect()
}

/// Serie autorregresiva de orden 1 y varianza 1:
/// `x(t) = φ·x(t−1) + √(1−φ²)·e(t)`.
///
/// Arranca en la distribución estacionaria, así que no hace falta descartar
/// un tramo inicial.
///
/// # Pánico
///
/// Si `|φ| ≥ 1`: la serie no sería estacionaria.
pub fn ar1(n: usize, phi: f64, generador: &mut Generador) -> Vec<f64> {
    assert!(phi.abs() < 1.0, "ar1: |phi| debe ser menor que 1");
    let escala = (1.0 - phi * phi).sqrt();
    let mut serie: Vec<f64> = Vec::with_capacity(n);
    for t in 0..n {
        let e = generador.normal();
        let valor = if t == 0 {
            e
        } else {
            phi * serie[t - 1] + escala * e
        };
        serie.push(valor);
    }
    serie
}

/// Observaciones armadas sobre una serie: características que miran hacia
/// atrás y una etiqueta que mira hacia adelante.
#[derive(Debug, Clone)]
pub struct Conjunto {
    /// Una fila por observación: el promedio de la serie en cada ventana.
    pub caracteristicas: Vec<Vec<f64>>,
    /// `+1` si la suma de los próximos valores es positiva; `−1` si no.
    pub etiquetas: Vec<f64>,
    /// De qué tramo de la serie depende cada observación.
    pub intervalos: Vec<Intervalo>,
}

/// Una observación por instante `t` con historia y futuro suficientes:
///
/// - una característica por ventana `w`: el promedio de los `w` valores que
///   terminan en `t`;
/// - la etiqueta: el signo de la suma de los `horizonte` valores siguientes;
/// - el intervalo: desde el primer valor de la ventana más larga hasta el
///   último del horizonte.
///
/// Las observaciones vecinas comparten casi toda la información, que es
/// justamente lo que hace falso al k-fold barajado.
///
/// # Pánico
///
/// Si `ventanas` está vacía o tiene un cero, o si `horizonte` es cero.
pub fn ventanas_pasadas(serie: &[f64], ventanas: &[usize], horizonte: usize) -> Conjunto {
    assert!(
        !ventanas.is_empty() && !ventanas.contains(&0),
        "ventanas_pasadas: ventanas inválidas"
    );
    assert!(
        horizonte > 0,
        "ventanas_pasadas: el horizonte debe ser al menos 1"
    );
    let mayor = *ventanas.iter().max().unwrap_or(&1);
    let mut conjunto = Conjunto {
        caracteristicas: Vec::new(),
        etiquetas: Vec::new(),
        intervalos: Vec::new(),
    };
    if serie.len() < mayor + horizonte {
        return conjunto;
    }
    for t in (mayor - 1)..(serie.len() - horizonte) {
        let fila = ventanas
            .iter()
            .map(|&w| suma(&serie[t + 1 - w..=t]) / w as f64)
            .collect();
        let futuro = suma(&serie[t + 1..=t + horizonte]);
        conjunto.caracteristicas.push(fila);
        conjunto
            .etiquetas
            .push(if futuro > 0.0 { 1.0 } else { -1.0 });
        conjunto.intervalos.push(
            Intervalo::nuevo((t + 1 - mayor) as u64, (t + horizonte) as u64).expect("inicio ≤ fin"),
        );
    }
    conjunto
}

/// Suma de izquierda a derecha: el orden fijo da el mismo resultado en
/// cualquier plataforma.
fn suma(valores: &[f64]) -> f64 {
    let mut total = 0.0;
    for v in valores {
        total += v;
    }
    total
}

/// Fracción de etiquetas acertadas por un clasificador de `k` vecinos más
/// cercanos, validando cada pliegue con un modelo que solo vio su
/// entrenamiento.
///
/// Las características se estandarizan con la media y el desvío del
/// entrenamiento de cada pliegue. La distancia es euclídea; ante un empate
/// gana el vecino de menor índice. `k` debería ser impar para que no haya
/// empates en la votación (un empate vota `−1`).
pub fn acierto_por_vecinos(conjunto: &Conjunto, pliegues: &[Pliegue], k: usize) -> f64 {
    let filas = &conjunto.caracteristicas;
    let dimension = filas.first().map_or(0, Vec::len);
    let (mut aciertos, mut validadas) = (0usize, 0usize);

    for pliegue in pliegues {
        if pliegue.entrenamiento.is_empty() {
            continue;
        }
        let m = pliegue.entrenamiento.len() as f64;
        let mut media = vec![0.0; dimension];
        let mut desvio = vec![0.0; dimension];
        for d in 0..dimension {
            let mut total = 0.0;
            for &j in &pliegue.entrenamiento {
                total += filas[j][d];
            }
            media[d] = total / m;
            let mut cuadrados = 0.0;
            for &j in &pliegue.entrenamiento {
                let desvio_j = filas[j][d] - media[d];
                cuadrados += desvio_j * desvio_j;
            }
            let s = (cuadrados / m).sqrt();
            desvio[d] = if s > 0.0 { s } else { 1.0 };
        }
        let estandarizar = |fila: &[f64]| -> Vec<f64> {
            fila.iter()
                .enumerate()
                .map(|(d, v)| (v - media[d]) / desvio[d])
                .collect()
        };
        let entrenamiento: Vec<Vec<f64>> = pliegue
            .entrenamiento
            .iter()
            .map(|&j| estandarizar(&filas[j]))
            .collect();

        for &i in &pliegue.validacion {
            let consulta = estandarizar(&filas[i]);
            // Los k mejores, ordenados por distancia; a igual distancia queda
            // el que llegó primero, que es el de menor índice.
            let mut mejores: Vec<(f64, usize)> = Vec::with_capacity(k + 1);
            for (posicion, fila) in entrenamiento.iter().enumerate() {
                let mut distancia = 0.0;
                for (a, b) in fila.iter().zip(&consulta) {
                    distancia += (a - b) * (a - b);
                }
                if mejores.len() < k || distancia < mejores[mejores.len() - 1].0 {
                    let lugar = mejores.partition_point(|&(d, _)| d <= distancia);
                    mejores.insert(lugar, (distancia, posicion));
                    mejores.truncate(k);
                }
            }
            let voto: f64 = mejores
                .iter()
                .map(|&(_, p)| conjunto.etiquetas[pliegue.entrenamiento[p]])
                .sum();
            let prediccion = if voto > 0.0 { 1.0 } else { -1.0 };
            if prediccion == conjunto.etiquetas[i] {
                aciertos += 1;
            }
            validadas += 1;
        }
    }
    if validadas == 0 {
        return f64::NAN;
    }
    aciertos as f64 / validadas as f64
}

/// Escenario con una señal débil escondida entre candidatas que no la tienen.
#[derive(Debug, Clone)]
pub struct SenalPlantada {
    /// Una columna por característica candidata: series AR(1) con φ = 0,7,
    /// independientes entre sí.
    pub candidatas: Vec<Vec<f64>>,
    /// `beta · candidatas[plantada] + ruido`, con ruido AR(1) de φ = 0,5.
    pub etiqueta: Vec<f64>,
    /// Cuál de las candidatas lleva la señal.
    pub plantada: usize,
}

/// Arma el escenario de [`SenalPlantada`].
///
/// Con `beta = 0,15` la correlación entre la candidata plantada y la etiqueta
/// ronda 0,15: explica alrededor del 2 % de la varianza. Las demás candidatas
/// no tienen ninguna relación con la etiqueta.
///
/// # Pánico
///
/// Si `plantada` no es una de las candidatas.
pub fn senal_plantada(
    n: usize,
    candidatas: usize,
    plantada: usize,
    beta: f64,
    generador: &mut Generador,
) -> SenalPlantada {
    assert!(
        plantada < candidatas,
        "senal_plantada: la candidata {plantada} no existe"
    );
    let columnas: Vec<Vec<f64>> = (0..candidatas).map(|_| ar1(n, 0.7, generador)).collect();
    let ruido = ar1(n, 0.5, generador);
    let etiqueta = columnas[plantada]
        .iter()
        .zip(&ruido)
        .map(|(c, e)| beta * c + e)
        .collect();
    SenalPlantada {
        candidatas: columnas,
        etiqueta,
        plantada,
    }
}
