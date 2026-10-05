//! La muestra sellada: compromiso, apertura única y separación.
//!
//! Los tests de apertura con marca escriben solo en directorios temporales
//! propios, bajo `std::env::temp_dir()`, y los borran al terminar.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use signal_validator::particion::{ErrorParticion, Intervalo};
use signal_validator::sellado::{separar, Compromiso, MotivoRechazo, MuestraSellada};

/// Directorio temporal propio, que se borra al soltarse.
struct DirTemporal(PathBuf);

impl DirTemporal {
    fn nuevo(nombre: &str) -> DirTemporal {
        static CONTADOR: AtomicUsize = AtomicUsize::new(0);
        let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
        let ruta = std::env::temp_dir().join(format!(
            "signal-validator-{nombre}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ruta);
        std::fs::create_dir_all(&ruta).expect("crear el directorio temporal");
        DirTemporal(ruta)
    }
}

impl Drop for DirTemporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// La codificación canónica, contra un SHA-256 calculado aparte (Python,
/// `struct` + `hashlib`) sobre los mismos bytes.
#[test]
fn el_compromiso_coincide_con_una_implementacion_externa() {
    let datos: Vec<(f64, u64)> = vec![(1.5, 2), (-0.0, 7), (f64::INFINITY, u64::MAX)];
    assert_eq!(
        MuestraSellada::sellar(datos).compromiso().to_string(),
        "1f7ed21b20cc5046f3fa67a421e1bf89f0ce6ea731e5c29301f183432657bc14"
    );
    let vacia: Vec<f64> = Vec::new();
    assert_eq!(
        Compromiso::de(&vacia).to_string(),
        "78bc3d1b22fd07b403f1ca7e6fcbce2daf0028b6fa84ac6815901f24d68d654f"
    );
    let anidada: Vec<Vec<f64>> = vec![vec![1.0, 2.0], vec![]];
    assert_eq!(
        Compromiso::de(&anidada).to_string(),
        "43d7f1f1e056a8f6e921e0fedcb436f6d28bf6eb82965f4caa041146af385312"
    );
}

#[test]
fn cualquier_cambio_en_los_datos_cambia_el_compromiso() {
    let base = Compromiso::de(&[1.0, 2.0, 3.0]);
    assert_eq!(base, Compromiso::de(&[1.0, 2.0, 3.0]));
    assert_ne!(base, Compromiso::de(&[1.0, 3.0, 2.0]), "el orden importa");
    assert_ne!(base, Compromiso::de(&[1.0, 2.0]), "el largo importa");
    assert_ne!(
        base,
        Compromiso::de(&[1.0, 2.0, f64::from_bits(3.0f64.to_bits() + 1)]),
        "un bit importa"
    );
    assert_ne!(
        Compromiso::de(&[0.0]),
        Compromiso::de(&[-0.0]),
        "0 y -0 son distintos"
    );
    // Dos agrupaciones de los mismos números no colisionan: el largo va adelante.
    assert_ne!(
        Compromiso::de(&[vec![1.0], vec![2.0, 3.0]]),
        Compromiso::de(&[vec![1.0, 2.0], vec![3.0]])
    );
    assert_eq!(
        MuestraSellada::sellar(vec![1.0, 2.0, 3.0]).compromiso(),
        base
    );
}

#[test]
fn abrir_entrega_los_datos_tal_cual() {
    let datos = vec![(1u64, 0.5), (2, -0.5)];
    let muestra = MuestraSellada::sellar(datos.clone());
    assert_eq!(muestra.len(), 2);
    assert!(!muestra.is_empty());
    assert_eq!(muestra.abrir(), datos);
}

#[test]
fn la_marca_impide_una_segunda_apertura() {
    let dir = DirTemporal::nuevo("marca");
    let marca = dir.0.join("reserva.abierta");
    let datos = vec![1.0, 2.0, 3.0];

    let primera = MuestraSellada::sellar(datos.clone());
    let compromiso = primera.compromiso();
    assert_eq!(
        primera.abrir_con_marca(&marca).expect("primera apertura"),
        datos
    );
    // La marca guarda el compromiso de lo que se abrió.
    let contenido = std::fs::read_to_string(&marca).expect("la marca existe");
    assert_eq!(contenido.trim(), compromiso.to_string());

    // Otra ejecución vuelve a sellar los mismos datos e intenta abrir.
    let error = MuestraSellada::sellar(datos)
        .abrir_con_marca(&marca)
        .expect_err("ya se abrió");
    assert!(matches!(error.motivo(), MotivoRechazo::YaAbierta(ruta) if *ruta == marca));
    // La muestra vuelve sellada: el error no destruye datos.
    let recuperada = error.recuperar();
    assert_eq!(recuperada.compromiso(), compromiso);
    assert_eq!(recuperada.len(), 3);
}

#[test]
fn si_no_se_puede_dejar_la_marca_no_se_abre() {
    let dir = DirTemporal::nuevo("sin-directorio");
    let marca = dir.0.join("no-existe").join("reserva.abierta");
    let error = MuestraSellada::sellar(vec![4.0, 5.0])
        .abrir_con_marca(&marca)
        .expect_err("sin directorio");
    assert!(matches!(error.motivo(), MotivoRechazo::Io(_)));
    assert!(!marca.exists());
    assert_eq!(error.recuperar().abrir(), vec![4.0, 5.0]);
}

/// Ocho hilos con la misma marca, liberados a la vez: abre exactamente uno.
#[test]
fn dos_ejecuciones_a_la_vez_no_pueden_abrir_las_dos() {
    let dir = DirTemporal::nuevo("carrera");
    let marca = Arc::new(dir.0.join("reserva.abierta"));
    let largada = Arc::new(Barrier::new(8));
    let hilos: Vec<_> = (0..8)
        .map(|_| {
            let (marca, largada) = (Arc::clone(&marca), Arc::clone(&largada));
            std::thread::spawn(move || {
                let muestra = MuestraSellada::sellar(vec![1.0, 2.0]);
                largada.wait();
                muestra.abrir_con_marca(marca.as_path()).is_ok()
            })
        })
        .collect();
    let abiertas = hilos
        .into_iter()
        .map(|h| h.join().expect("el hilo no entra en pánico"))
        .filter(|&ok| ok)
        .count();
    assert_eq!(abiertas, 1);
}

#[test]
fn el_debug_no_muestra_los_datos() {
    let muestra = MuestraSellada::sellar(vec![12345.678, 98765.4321]);
    let texto = format!("{muestra:?}");
    assert!(
        !texto.contains("12345") && !texto.contains("98765"),
        "{texto}"
    );
    assert!(texto.contains("observaciones: 2"), "{texto}");
}

#[test]
fn separar_purga_del_diseno_lo_que_toca_la_reserva() {
    // Cada observación depende de su instante y de los tres siguientes.
    let datos: Vec<u64> = (0..20).collect();
    let intervalos: Vec<Intervalo> = (0..20)
        .map(|t| Intervalo::nuevo(t, t + 3).expect("válido"))
        .collect();
    let separacion = separar(datos.clone(), &intervalos, 15).expect("válida");

    // La reserva empieza en el instante 15: 12, 13 y 14 llegan hasta él.
    assert_eq!(separacion.diseno, (0..12).collect::<Vec<u64>>());
    assert_eq!(separacion.purgadas, 3);
    assert_eq!(
        separacion.reserva.compromiso(),
        Compromiso::de(&datos[15..])
    );
    assert_eq!(separacion.reserva.abrir(), (15..20).collect::<Vec<u64>>());
}

#[test]
fn separar_rechaza_entradas_invalidas() {
    let datos: Vec<u64> = (0..5).collect();
    let intervalos: Vec<Intervalo> = (0..5)
        .map(|t| Intervalo::nuevo(t, t).expect("válido"))
        .collect();
    assert!(matches!(
        separar(datos.clone(), &intervalos, 0),
        Err(ErrorParticion::CorteInvalido { .. })
    ));
    assert!(matches!(
        separar(datos.clone(), &intervalos, 5),
        Err(ErrorParticion::CorteInvalido { .. })
    ));
    assert!(matches!(
        separar(datos.clone(), &intervalos[..4], 2),
        Err(ErrorParticion::LargosDistintos {
            observaciones: 5,
            intervalos: 4
        })
    ));
    // Inicios 0, 3, 2, 1, 4: el primero que empieza antes que el anterior es el 2.
    let mut desordenados = intervalos.clone();
    desordenados.swap(1, 3);
    assert!(matches!(
        separar(datos, &desordenados, 2),
        Err(ErrorParticion::FueraDeOrden { indice: 2 })
    ));
}
