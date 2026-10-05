//! El almacén en disco: atomicidad, nombres, retroceso y archivos manipulados.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use sealed_checkpoint::{Almacen, Clave, ErrorAlmacen, ErrorSello};

static N: AtomicU32 = AtomicU32::new(0);

/// Un directorio temporal propio por test, que se borra al soltarse.
struct Dir(PathBuf);

impl Dir {
    fn nuevo(etiqueta: &str) -> Self {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("sck-{etiqueta}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn almacen(dir: &Dir) -> Almacen {
    Almacen::abrir(&dir.0, Clave::desde_bytes([7; 32])).expect("abrir almacén")
}

fn archivos(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .expect("listar")
        .map(|e| {
            e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    v.sort();
    v
}

#[test]
fn guardar_y_cargar() {
    let d = Dir::nuevo("basico");
    let a = almacen(&d);
    assert!(
        a.cargar("sesion").expect("cargar").is_none(),
        "sin checkpoint todavía"
    );
    a.guardar("sesion", 1, b"v1").expect("guardar");
    a.guardar("sesion", 2, b"v2").expect("guardar");
    let c = a.cargar("sesion").expect("cargar").expect("existe");
    assert_eq!((c.generacion, c.datos.as_slice()), (2, &b"v2"[..]));
}

#[test]
fn no_quedan_temporales_despues_de_guardar() {
    let d = Dir::nuevo("temporales");
    let a = almacen(&d);
    for g in 1..=20 {
        a.guardar("sesion", g, format!("estado {g}").as_bytes())
            .expect("guardar");
    }
    assert_eq!(archivos(&d.0), vec!["sesion.sck".to_string()]);
}

#[test]
fn un_temporal_huerfano_de_una_caida_no_afecta_la_carga() {
    // Simula un proceso que murió entre escribir el temporal y hacer el rename.
    let d = Dir::nuevo("huerfano");
    let a = almacen(&d);
    a.guardar("sesion", 1, b"confirmado").expect("guardar");
    fs::write(d.0.join(".sesion.99999.0.tmp"), b"escritura a medias").expect("temporal");

    let c = a.cargar("sesion").expect("cargar").expect("existe");
    assert_eq!(c.datos.as_slice(), b"confirmado");
    a.guardar("sesion", 2, b"siguiente")
        .expect("guardar con un huérfano presente");
}

#[test]
fn los_nombres_que_podrian_escapar_del_directorio_se_rechazan() {
    let d = Dir::nuevo("nombres");
    let a = almacen(&d);
    let largo = "a".repeat(65);
    for malo in [
        "",
        "../fuera",
        "a/b",
        "a\\b",
        "Mayus",
        ".oculto",
        "con espacio",
        largo.as_str(),
    ] {
        assert!(
            matches!(
                a.guardar(malo, 1, b"x"),
                Err(ErrorAlmacen::NombreInvalido(_))
            ),
            "`{malo}`"
        );
        assert!(
            matches!(a.cargar(malo), Err(ErrorAlmacen::NombreInvalido(_))),
            "`{malo}`"
        );
    }
    assert!(archivos(&d.0).is_empty(), "no se escribió nada");
}

#[test]
fn una_generacion_que_no_avanza_se_rechaza_al_guardar() {
    let d = Dir::nuevo("no-avanza");
    let a = almacen(&d);
    a.guardar("sesion", 5, b"cinco").expect("guardar");
    for g in [5, 3, 0] {
        match a.guardar("sesion", g, b"viejo") {
            Err(ErrorAlmacen::GeneracionNoAvanza {
                existente: 5,
                pedida,
            }) => assert_eq!(pedida, g),
            otro => panic!("generación {g}: {otro:?}"),
        }
    }
    assert_eq!(
        a.cargar("sesion")
            .expect("cargar")
            .expect("existe")
            .datos
            .as_slice(),
        b"cinco"
    );
}

#[test]
fn reemplazar_el_archivo_por_una_copia_vieja_se_detecta_con_la_minima() {
    let d = Dir::nuevo("retroceso");
    let a = almacen(&d);
    let ruta = a.ruta_de("sesion").expect("ruta");
    a.guardar("sesion", 1, b"viejo").expect("guardar");
    let copia_vieja = fs::read(&ruta).expect("leer");
    a.guardar("sesion", 2, b"nuevo").expect("guardar");

    // Un atacante con acceso al disco repone la copia vieja: sigue siendo auténtica.
    fs::write(&ruta, &copia_vieja).expect("reponer");
    assert_eq!(
        a.cargar("sesion")
            .expect("cargar")
            .expect("existe")
            .generacion,
        1,
        "el sello no lo delata…"
    );
    match a.cargar_desde("sesion", 2) {
        Err(ErrorAlmacen::Retroceso {
            encontrada: 1,
            minima: 2,
        }) => {}
        otro => panic!("…pero la mínima conocida sí: {otro:?}"),
    }
}

#[test]
fn un_checkpoint_renombrado_no_abre_con_otro_nombre() {
    let d = Dir::nuevo("renombrado");
    let a = almacen(&d);
    a.guardar("sesion", 1, b"de sesion").expect("guardar");
    fs::copy(
        a.ruta_de("sesion").expect("ruta"),
        a.ruta_de("config").expect("ruta"),
    )
    .expect("copiar");
    assert!(matches!(
        a.cargar("config"),
        Err(ErrorAlmacen::Sello(ErrorSello::Autenticacion))
    ));
}

#[test]
fn un_archivo_corrupto_da_error_y_se_puede_reemplazar() {
    let d = Dir::nuevo("corrupto");
    let a = almacen(&d);
    a.guardar("sesion", 4, b"bueno").expect("guardar");
    let ruta = a.ruta_de("sesion").expect("ruta");
    let mut bytes = fs::read(&ruta).expect("leer");
    let ultimo = bytes.len() - 1;
    bytes[ultimo] ^= 0xFF;
    fs::write(&ruta, &bytes).expect("corromper");

    assert!(
        matches!(
            a.cargar("sesion"),
            Err(ErrorAlmacen::Sello(ErrorSello::Autenticacion))
        ),
        "error, nunca basura"
    );
    // No se puede exigir que avance sobre algo ilegible: se recupera escribiendo de nuevo.
    a.guardar("sesion", 1, b"recuperado")
        .expect("reemplazar el corrupto");
    assert_eq!(
        a.cargar("sesion")
            .expect("cargar")
            .expect("existe")
            .datos
            .as_slice(),
        b"recuperado"
    );
}

#[test]
fn con_otra_clave_no_abre() {
    let d = Dir::nuevo("otra-clave");
    almacen(&d).guardar("sesion", 1, b"x").expect("guardar");
    let ajeno = Almacen::abrir(&d.0, Clave::desde_bytes([8; 32])).expect("abrir");
    assert!(matches!(
        ajeno.cargar("sesion"),
        Err(ErrorAlmacen::Sello(ErrorSello::Autenticacion))
    ));
}
