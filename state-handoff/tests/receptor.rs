//! El receptor: la cerca de época (con secuencias pseudoaleatorias fijas), la confirmación en
//! dos fases, el borrado al confirmar y el aborto que no consume la época.

use state_handoff::paquete::sellar;
use state_handoff::receptor::Receptor;
use state_handoff::Error;

const ACTIVO: u16 = 1;
const RESPALDO: u16 = 2;
const CAP: usize = 128;

/// Una tabla de calibración de prueba: 16 bytes que dependen de la época.
fn tabla(epoca: u64) -> [u8; 16] {
    core::array::from_fn(|i| (epoca as u8).wrapping_mul(31).wrapping_add(i as u8))
}

fn paquete(epoca: u64) -> Vec<u8> {
    let mut b = vec![0u8; CAP];
    let n = sellar(&tabla(epoca), ACTIVO, RESPALDO, epoca, &mut b).unwrap();
    b.truncate(n);
    b
}

/// Carga el paquete de `epoca` y lo confirma. Devuelve lo que vio el respaldo.
fn traspasar(respaldo: &mut Receptor<CAP>, epoca: u64) -> Result<Vec<u8>, Error> {
    respaldo.cargar(&paquete(epoca))?;
    let preparado = respaldo.preparar()?;
    let visto = preparado.contenido().to_vec();
    preparado.confirmar();
    Ok(visto)
}

#[test]
fn preparar_y_confirmar_instala_la_tabla_y_registra_la_epoca() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    assert_eq!(respaldo.ultima_epoca(), 0);
    respaldo.cargar(&paquete(7)).unwrap();
    let preparado = respaldo.preparar().unwrap();
    assert_eq!(preparado.epoca(), 7);
    assert_eq!(preparado.contenido(), &tabla(7));
    preparado.confirmar();
    assert_eq!(respaldo.ultima_epoca(), 7);
}

#[test]
fn confirmar_borra_el_area_de_preparacion() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    respaldo.cargar(&paquete(3)).unwrap();
    let preparado = respaldo.preparar().unwrap();
    // Antes de confirmar el área tiene el paquete: la comprobación de abajo no es trivial.
    assert!(preparado.contenido().iter().any(|&b| b != 0));
    preparado.confirmar();
    assert!(respaldo.area_de_preparacion().iter().all(|&b| b == 0));
}

#[test]
fn una_repeticion_o_una_epoca_vieja_se_rechazan() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    traspasar(&mut respaldo, 7).unwrap();
    assert_eq!(
        traspasar(&mut respaldo, 7),
        Err(Error::EpocaVieja {
            ultima: 7,
            recibida: 7
        })
    );
    assert_eq!(
        traspasar(&mut respaldo, 5),
        Err(Error::EpocaVieja {
            ultima: 7,
            recibida: 5
        })
    );
    assert_eq!(respaldo.ultima_epoca(), 7);
    assert_eq!(traspasar(&mut respaldo, 8), Ok(tabla(8).to_vec()));
}

#[test]
fn la_epoca_cero_no_pasa_nunca() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    assert_eq!(
        traspasar(&mut respaldo, 0),
        Err(Error::EpocaVieja {
            ultima: 0,
            recibida: 0
        })
    );
}

#[test]
fn abortar_no_consume_la_epoca() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    respaldo.cargar(&paquete(4)).unwrap();
    respaldo.preparar().unwrap().abortar();
    assert_eq!(respaldo.ultima_epoca(), 0);
    // El área quedó como estaba: el mismo paquete se vuelve a preparar, y ahora se confirma.
    let preparado = respaldo.preparar().unwrap();
    assert_eq!(preparado.contenido(), &tabla(4));
    preparado.confirmar();
    assert_eq!(respaldo.ultima_epoca(), 4);
}

#[test]
fn soltar_el_preparado_sin_confirmar_es_abortar() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    respaldo.cargar(&paquete(4)).unwrap();
    {
        let _preparado = respaldo.preparar().unwrap();
    }
    assert_eq!(respaldo.ultima_epoca(), 0);
    assert!(respaldo.preparar().is_ok());
}

#[test]
fn un_error_de_verificacion_no_toca_la_epoca() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    traspasar(&mut respaldo, 2).unwrap();
    let mut roto = paquete(9);
    let ultimo = roto.len() - 1;
    roto[ultimo] ^= 0x40;
    respaldo.cargar(&roto).unwrap();
    assert_eq!(respaldo.preparar().unwrap_err(), Error::ContenidoCorrupto);
    let mut ajeno = vec![0u8; CAP];
    let n = sellar(&[1], ACTIVO, 9, 10, &mut ajeno).unwrap();
    respaldo.cargar(&ajeno[..n]).unwrap();
    assert_eq!(respaldo.preparar().unwrap_err(), Error::DestinoAjeno(9));
    assert_eq!(respaldo.ultima_epoca(), 2);
}

#[test]
fn cargar_lo_que_no_cabe_es_un_error_y_no_toca_el_area() {
    let mut respaldo = Receptor::<32>::nuevo(RESPALDO);
    let b = paquete(1);
    assert!(b.len() > 32);
    assert_eq!(respaldo.cargar(&b), Err(Error::NoCabe));
    assert_eq!(respaldo.area_de_preparacion(), &[0; 32]);
}

#[test]
fn cargar_borra_lo_que_quedaba_de_una_carga_anterior() {
    let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
    let largo = paquete(1);
    respaldo.cargar(&largo).unwrap();
    respaldo.cargar(&largo[..10]).unwrap();
    assert!(respaldo.area_de_preparacion()[10..].iter().all(|&b| b == 0));
    assert_eq!(&respaldo.area_de_preparacion()[..10], &largo[..10]);
}

#[test]
fn un_receptor_reiniciado_recuerda_su_cerca() {
    let mut respaldo = Receptor::<CAP>::con_epoca(RESPALDO, 10);
    assert!(traspasar(&mut respaldo, 10).is_err());
    assert!(traspasar(&mut respaldo, 11).is_ok());
}

/// SplitMix64, con semilla fija.
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[test]
fn con_secuencias_pseudoaleatorias_las_epocas_aceptadas_crecen_estrictamente() {
    // Cada secuencia mezcla épocas nuevas, repetidas y viejas (al azar en una ventana que
    // avanza despacio), y aborta un tercio de lo que pasa la cerca. El modelo es una línea:
    // pasa si y solo si es mayor que la última confirmada.
    for semilla in [1u64, 2, 3] {
        let mut azar = Azar(semilla);
        let mut respaldo = Receptor::<CAP>::nuevo(RESPALDO);
        let mut ultima = 0u64;
        let mut aceptadas = Vec::new();
        let (mut rechazadas, mut abortadas) = (0, 0);
        for i in 0..2000u64 {
            let epoca = i / 4 + azar.siguiente() % 24;
            respaldo.cargar(&paquete(epoca)).unwrap();
            match respaldo.preparar() {
                Ok(preparado) => {
                    assert!(epoca > ultima, "pasó {epoca} con la última en {ultima}");
                    if azar.siguiente().is_multiple_of(3) {
                        preparado.abortar();
                        abortadas += 1;
                    } else {
                        preparado.confirmar();
                        ultima = epoca;
                        aceptadas.push(epoca);
                    }
                }
                Err(e) => {
                    assert_eq!(
                        e,
                        Error::EpocaVieja {
                            ultima,
                            recibida: epoca
                        }
                    );
                    rechazadas += 1;
                }
            }
            assert_eq!(respaldo.ultima_epoca(), ultima);
        }
        assert!(aceptadas.windows(2).all(|par| par[0] < par[1]));
        // La secuencia ejercita los tres caminos.
        assert!(aceptadas.len() > 50 && rechazadas > 500 && abortadas > 20);
        println!(
            "semilla {semilla}: {} aceptadas, {rechazadas} rechazadas, {abortadas} abortadas",
            aceptadas.len()
        );
    }
}
