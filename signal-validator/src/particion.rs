//! Partición en pliegues para validación cruzada sobre series temporales.
//!
//! En una serie, las observaciones vecinas se solapan: una característica
//! calculada sobre una ventana pasada, una etiqueta sobre un horizonte futuro.
//! Si los pliegues se arman al azar, la validación termina viendo, casi
//! idénticas, observaciones que el modelo ya vio al entrenar. Sobre ruido puro
//! eso alcanza para «encontrar» una señal que no existe (ver
//! [`crate::sintetico`] y la demo).
//!
//! [`particionar`] corta la serie en bloques contiguos y, para cada bloque de
//! validación:
//!
//! - **purga** del entrenamiento toda observación cuyo [`Intervalo`] de
//!   información se cruza con lo que abarca el bloque;
//! - **embarga** además las que empiezan poco después del bloque. El embargo
//!   cubre la memoria que los intervalos no declaran —un promedio exponencial,
//!   un acumulado— y que arrastra información del bloque hacia adelante.
//!
//! Dejar un margen alrededor del bloque de validación es la idea de la
//! validación cruzada para datos dependientes: *h-block* (Burman, Chow y Nolan,
//! 1994, *Biometrika*) y *hv-block* (Racine, 2000, *Journal of Econometrics*).

use std::fmt;

use crate::azar::Generador;

/// Tramo de tiempo, cerrado en ambos extremos, del que depende una
/// observación: desde el primer dato que usan sus características hasta el
/// último que usa su etiqueta.
///
/// La unidad la elige quien llama (índices, segundos, días); solo tiene que
/// ser la misma que la del embargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intervalo {
    inicio: u64,
    fin: u64,
}

impl Intervalo {
    /// `[inicio, fin]`, o `None` si `fin < inicio`.
    pub fn nuevo(inicio: u64, fin: u64) -> Option<Intervalo> {
        (inicio <= fin).then_some(Intervalo { inicio, fin })
    }

    /// Primer instante del que depende la observación.
    pub fn inicio(&self) -> u64 {
        self.inicio
    }

    /// Último instante del que depende la observación.
    pub fn fin(&self) -> u64 {
        self.fin
    }

    /// `true` si los dos intervalos comparten al menos un instante.
    pub fn se_cruza_con(&self, otro: &Intervalo) -> bool {
        self.inicio <= otro.fin && otro.inicio <= self.fin
    }
}

/// Un pliegue: qué observaciones se validan y con cuáles se entrena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pliegue {
    /// Índices de validación, en orden creciente.
    pub validacion: Vec<usize>,
    /// Índices de entrenamiento, en orden creciente.
    pub entrenamiento: Vec<usize>,
    /// Observaciones apartadas del entrenamiento por cruzarse con la validación.
    pub purgadas: usize,
    /// Observaciones apartadas por el embargo.
    pub embargadas: usize,
}

/// Por qué no se pudo particionar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorParticion {
    /// Hacen falta al menos dos pliegues.
    PocosPliegues {
        /// Los pliegues pedidos.
        pliegues: usize,
    },
    /// Algún pliegue quedaría sin observaciones de validación.
    MasPlieguesQueObservaciones {
        /// Los pliegues pedidos.
        pliegues: usize,
        /// Las observaciones disponibles.
        observaciones: usize,
    },
    /// Los intervalos no están en orden temporal: el de `indice` empieza antes
    /// que el anterior. Sin orden, un bloque contiguo de índices no es un tramo
    /// contiguo de tiempo.
    FueraDeOrden {
        /// El primer intervalo fuera de orden.
        indice: usize,
    },
    /// Hay distinta cantidad de observaciones que de intervalos.
    LargosDistintos {
        /// Las observaciones.
        observaciones: usize,
        /// Los intervalos.
        intervalos: usize,
    },
    /// El corte entre diseño y reserva deja vacía una de las dos partes.
    CorteInvalido {
        /// El corte pedido.
        corte: usize,
        /// Las observaciones disponibles.
        observaciones: usize,
    },
}

impl fmt::Display for ErrorParticion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorParticion::PocosPliegues { pliegues } => {
                write!(f, "hacen falta al menos 2 pliegues; se pidieron {pliegues}")
            }
            ErrorParticion::MasPlieguesQueObservaciones { pliegues, observaciones } => write!(
                f,
                "{pliegues} pliegues no caben en {observaciones} observaciones: alguno quedaría vacío"
            ),
            ErrorParticion::FueraDeOrden { indice } => write!(
                f,
                "el intervalo {indice} empieza antes que el anterior: las observaciones deben venir en orden temporal"
            ),
            ErrorParticion::LargosDistintos { observaciones, intervalos } => {
                write!(f, "{observaciones} observaciones pero {intervalos} intervalos")
            }
            ErrorParticion::CorteInvalido { corte, observaciones } => write!(
                f,
                "el corte {corte} sobre {observaciones} observaciones deja vacío el diseño o la reserva"
            ),
        }
    }
}

impl std::error::Error for ErrorParticion {}

/// Comprueba que los intervalos vengan ordenados por inicio.
pub(crate) fn exigir_orden(intervalos: &[Intervalo]) -> Result<(), ErrorParticion> {
    match (1..intervalos.len()).find(|&i| intervalos[i].inicio < intervalos[i - 1].inicio) {
        Some(indice) => Err(ErrorParticion::FueraDeOrden { indice }),
        None => Ok(()),
    }
}

/// Validación cruzada por bloques contiguos, con purga y embargo.
///
/// Corta las observaciones en `pliegues` bloques contiguos de tamaño casi
/// igual. Para cada bloque, el entrenamiento es todo lo demás **menos**:
///
/// - las observaciones cuyo intervalo se cruza con lo que abarca el bloque
///   (desde el inicio más temprano hasta el fin más tardío de sus intervalos);
/// - las que empiezan después del bloque, hasta `embargo` unidades más allá de
///   ese fin.
///
/// Se purga contra lo que abarca el bloque entero, no observación por
/// observación: es más simple de razonar y nunca purga de menos.
///
/// `intervalos` debe venir en orden temporal (por inicio).
///
/// ```
/// use signal_validator::particion::{particionar, Intervalo};
///
/// // Diez observaciones; cada una depende de su instante y de los dos siguientes.
/// let intervalos: Vec<Intervalo> = (0..10).map(|t| Intervalo::nuevo(t, t + 2).unwrap()).collect();
/// let pliegues = particionar(&intervalos, 2, 1).unwrap();
///
/// // Validación 0..5 abarca [0, 6]: se purgan 5 y 6, y el embargo aparta el 7.
/// assert_eq!(pliegues[0].validacion, vec![0, 1, 2, 3, 4]);
/// assert_eq!(pliegues[0].entrenamiento, vec![8, 9]);
/// ```
pub fn particionar(
    intervalos: &[Intervalo],
    pliegues: usize,
    embargo: u64,
) -> Result<Vec<Pliegue>, ErrorParticion> {
    let n = intervalos.len();
    if pliegues < 2 {
        return Err(ErrorParticion::PocosPliegues { pliegues });
    }
    if pliegues > n {
        return Err(ErrorParticion::MasPlieguesQueObservaciones {
            pliegues,
            observaciones: n,
        });
    }
    exigir_orden(intervalos)?;

    let mut resultado = Vec::with_capacity(pliegues);
    for f in 0..pliegues {
        let bloque = (f * n / pliegues)..((f + 1) * n / pliegues);
        let tramo = &intervalos[bloque.clone()];
        // Ningún bloque está vacío porque pliegues ≤ n.
        let abarcado = Intervalo {
            inicio: tramo.iter().map(|i| i.inicio).min().unwrap_or(0),
            fin: tramo.iter().map(|i| i.fin).max().unwrap_or(0),
        };
        let fin_del_embargo = abarcado.fin.saturating_add(embargo);

        let mut pliegue = Pliegue {
            validacion: bloque.clone().collect(),
            entrenamiento: Vec::with_capacity(n - bloque.len()),
            purgadas: 0,
            embargadas: 0,
        };
        for (j, intervalo) in intervalos.iter().enumerate() {
            if bloque.contains(&j) {
                continue;
            }
            if intervalo.se_cruza_con(&abarcado) {
                pliegue.purgadas += 1;
            } else if intervalo.inicio > abarcado.fin && intervalo.inicio <= fin_del_embargo {
                pliegue.embargadas += 1;
            } else {
                pliegue.entrenamiento.push(j);
            }
        }
        resultado.push(pliegue);
    }
    Ok(resultado)
}

/// La partición ingenua, **para comparar**: reparte las observaciones entre
/// los pliegues al azar, sin purga ni embargo.
///
/// Es lo que no hay que hacer con series temporales. Está aquí para que la
/// demo y los tests puedan mostrar, sobre los mismos datos, cuánto se equivoca.
pub fn kfold_barajado(
    n: usize,
    pliegues: usize,
    generador: &mut Generador,
) -> Result<Vec<Pliegue>, ErrorParticion> {
    if pliegues < 2 {
        return Err(ErrorParticion::PocosPliegues { pliegues });
    }
    if pliegues > n {
        return Err(ErrorParticion::MasPlieguesQueObservaciones {
            pliegues,
            observaciones: n,
        });
    }
    let mut orden: Vec<usize> = (0..n).collect();
    generador.barajar(&mut orden);

    let mut resultado = Vec::with_capacity(pliegues);
    for f in 0..pliegues {
        let mut validacion = orden[f * n / pliegues..(f + 1) * n / pliegues].to_vec();
        validacion.sort_unstable();
        let mut en_validacion = vec![false; n];
        for &i in &validacion {
            en_validacion[i] = true;
        }
        let entrenamiento = (0..n).filter(|&i| !en_validacion[i]).collect();
        resultado.push(Pliegue {
            validacion,
            entrenamiento,
            purgadas: 0,
            embargadas: 0,
        });
    }
    Ok(resultado)
}
