//! Las rutas del plan no salen de la raíz.

mod comun;

use comun::Repo;
use staged_agent::rutas::{resolver_dentro, ErrorRuta, RutaRelativa};

#[test]
fn se_rechazan_las_rutas_que_podrian_salir_de_la_raiz() {
    let malas = [
        "",
        "..",
        "../afuera.txt",
        "a/../../b",
        "a/..",
        ".",
        "./a",
        "a/./b",
        "/etc/hosts",
        "a//b",
        "a/",
        "C:\\Windows\\win.ini",
        "C:archivo",
        "\\\\servidor\\recurso",
        "a\\b",
        "a\0b",
        "archivo.txt:flujo",
    ];
    for mala in malas {
        assert!(
            RutaRelativa::nueva(mala).is_err(),
            "{mala:?} debería rechazarse"
        );
    }
}

#[test]
fn se_aceptan_las_rutas_normales() {
    for buena in [
        "a.txt",
        "docs/guia.md",
        "con espacios.txt",
        "ñandú/ü.md",
        ".gitignore",
        "a/.oculto/b",
        "...",
        "a..b",
    ] {
        let ruta = RutaRelativa::nueva(buena).unwrap_or_else(|e| panic!("{buena:?}: {e}"));
        assert_eq!(ruta.como_str(), buena);
    }
}

#[test]
fn resolver_dentro_encuentra_lo_que_existe_y_nada_mas() {
    let repo = Repo::nuevo("rutas-resolver");
    repo.escribir("docs/guia.md", b"hola\n");
    let ruta = RutaRelativa::nueva("docs/guia.md").expect("válida");
    let resuelta = resolver_dentro(&repo.raiz, &ruta).expect("existe y está adentro");
    assert!(resuelta.ends_with("guia.md"));
    assert!(resuelta.starts_with(repo.raiz.canonicalize().expect("canónica")));

    let ausente = RutaRelativa::nueva("docs/no-existe.md").expect("válida");
    assert!(matches!(
        resolver_dentro(&repo.raiz, &ausente),
        Err(ErrorRuta::NoExiste(_))
    ));
}

/// Un directorio del árbol que es un enlace hacia afuera: escribir «adentro»
/// sería escribir afuera.
#[cfg(unix)]
#[test]
fn un_directorio_enlazado_hacia_afuera_se_rechaza() {
    use std::os::unix::fs::symlink;

    let repo = Repo::nuevo("rutas-enlace-afuera");
    let afuera = repo.directorio_aparte("afuera");
    std::fs::write(afuera.join("secreto.txt"), b"no tocar\n").expect("escribir");
    symlink(&afuera, repo.raiz.join("enlace")).expect("crear el enlace");

    let ruta = RutaRelativa::nueva("enlace/secreto.txt").expect("válida como texto");
    assert!(matches!(
        resolver_dentro(&repo.raiz, &ruta),
        Err(ErrorRuta::EnlaceSimbolico(_))
    ));
}

/// Un archivo que es un enlace se rechaza aunque apunte adentro: el plan diría
/// un archivo y se escribiría otro.
#[cfg(unix)]
#[test]
fn un_archivo_enlazado_se_rechaza_aunque_apunte_adentro() {
    use std::os::unix::fs::symlink;

    let repo = Repo::nuevo("rutas-enlace-adentro");
    repo.escribir("real.txt", b"real\n");
    symlink("real.txt", repo.raiz.join("alias.txt")).expect("crear el enlace");

    let ruta = RutaRelativa::nueva("alias.txt").expect("válida como texto");
    assert!(matches!(
        resolver_dentro(&repo.raiz, &ruta),
        Err(ErrorRuta::EnlaceSimbolico(_))
    ));
}

/// La raíz sí puede llegar por un enlace: lo que se exige es que, desde la raíz
/// canónica, la ruta no pase por ninguno.
#[cfg(unix)]
#[test]
fn la_raiz_puede_ser_un_enlace() {
    use std::os::unix::fs::symlink;

    let repo = Repo::nuevo("rutas-raiz-enlazada");
    repo.escribir("a.txt", b"a\n");
    let atajo = repo.directorio_aparte("atajos").join("al-repo");
    symlink(&repo.raiz, &atajo).expect("crear el enlace");

    let ruta = RutaRelativa::nueva("a.txt").expect("válida");
    assert!(resolver_dentro(&atajo, &ruta).is_ok());
}
