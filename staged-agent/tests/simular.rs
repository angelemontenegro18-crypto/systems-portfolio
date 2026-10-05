//! Nivel 1: simular calcula el plan sin escribir nada.

mod comun;

use comun::Repo;
use staged_agent::{Agente, Error, FinesDeLinea, Omision};

/// Un repositorio con un archivo de cada caso.
fn repo_de_ejemplo(nombre: &str) -> Repo {
    let repo = Repo::nuevo(nombre);
    repo.escribir("limpio.md", b"todo bien\n");
    repo.escribir("espacios.md", b"linea con espacios   \notra\t\n");
    repo.escribir("crlf.txt", b"uno\r\ndos\r\n");
    repo.escribir("sin_salto.txt", b"sin salto final");
    repo.escribir("docs/anidado.md", b"anidado  \n");
    repo.escribir("datos.bin", b"\x00\x01\x02 \n");
    repo.commit_todo("inicial");
    repo
}

#[test]
fn simular_no_escribe_nada() {
    let repo = repo_de_ejemplo("simular-no-escribe");
    let antes = repo.huella_completa();
    let plan = repo.agente().simular("main").expect("simular");
    assert!(!plan.esta_vacio());
    assert_eq!(
        repo.huella_completa(),
        antes,
        "simular cambió algo, en el árbol o en .git"
    );
}

#[test]
fn el_plan_propone_exactamente_lo_que_cambia() {
    let repo = repo_de_ejemplo("simular-plan");
    let plan = repo.agente().simular("main").expect("simular");
    assert_eq!(plan.base(), repo.punta("main"));
    assert_eq!(plan.destino(), "main");

    let rutas: Vec<&str> = plan.cambios().iter().map(|c| c.ruta().como_str()).collect();
    assert_eq!(
        rutas,
        [
            "crlf.txt",
            "docs/anidado.md",
            "espacios.md",
            "sin_salto.txt"
        ]
    );

    let cambio = |ruta: &str| {
        plan.cambios()
            .iter()
            .find(|c| c.ruta().como_str() == ruta)
            .expect("en el plan")
    };
    assert_eq!(cambio("crlf.txt").despues(), b"uno\ndos\n");
    assert_eq!(cambio("crlf.txt").reglas(), ["finales de línea"]);
    assert_eq!(
        cambio("espacios.md").despues(),
        b"linea con espacios\notra\n"
    );
    assert_eq!(cambio("espacios.md").reglas(), ["espacios finales"]);
    assert_eq!(cambio("sin_salto.txt").despues(), b"sin salto final\n");
    assert_eq!(cambio("sin_salto.txt").antes(), b"sin salto final");

    assert_eq!(plan.omitidos().len(), 1);
    assert_eq!(plan.omitidos()[0].ruta, "datos.bin");
    assert_eq!(plan.omitidos()[0].motivo, Omision::Binario);
}

/// Lo que se simula es lo commiteado: lo que haya sin commitear en el árbol de
/// trabajo no entra en el plan.
#[test]
fn simular_lee_lo_commiteado_no_el_arbol_de_trabajo() {
    let repo = repo_de_ejemplo("simular-commiteado");
    repo.escribir("limpio.md", b"ahora con espacios   \n");
    repo.escribir("espacios.md", b"ya arreglado a mano\n");
    let plan = repo.agente().simular("main").expect("simular");
    let rutas: Vec<&str> = plan.cambios().iter().map(|c| c.ruta().como_str()).collect();
    assert!(!rutas.contains(&"limpio.md"));
    let espacios = plan
        .cambios()
        .iter()
        .find(|c| c.ruta().como_str() == "espacios.md")
        .expect("en el plan");
    assert_eq!(espacios.antes(), b"linea con espacios   \notra\t\n");
}

#[test]
fn la_huella_es_determinista_y_depende_de_todo_el_plan() {
    let repo = repo_de_ejemplo("simular-huella");
    let agente = repo.agente();
    let primera = agente.simular("main").expect("simular");
    assert_eq!(
        agente.simular("main").expect("simular").huella(),
        primera.huella()
    );

    // Otras reglas, otro después: otra huella.
    let solo_crlf = Agente::nuevo(&repo.raiz)
        .con_git(repo.git_aislado())
        .con_reglas(vec![Box::new(FinesDeLinea)]);
    assert_ne!(
        solo_crlf.simular("main").expect("simular").huella(),
        primera.huella()
    );

    // Otra base, aunque el plan cambie los mismos archivos igual: otra huella.
    repo.escribir("nuevo-limpio.md", b"limpio\n");
    repo.commit_todo("otro commit");
    let segunda = agente.simular("main").expect("simular");
    assert_eq!(segunda.cambios(), primera.cambios());
    assert_ne!(segunda.huella(), primera.huella());
}

#[test]
fn las_ramas_inexistentes_o_peligrosas_se_rechazan() {
    let repo = repo_de_ejemplo("simular-ramas");
    let agente = repo.agente();
    assert!(matches!(
        agente.simular("no-existe"),
        Err(Error::RamaInexistente(_))
    ));
    for peligrosa in [
        "",
        "-x",
        "--output=pwned",
        "../main",
        "a..b",
        "main.lock",
        "/main",
        "main/",
        "a//b",
        "ma in",
    ] {
        assert!(
            matches!(
                agente.simular(peligrosa),
                Err(Error::NombreDeRamaInvalido(_))
            ),
            "{peligrosa:?} debería rechazarse"
        );
    }
}

/// Un enlace simbólico del repositorio no se normaliza: modificarlo tocaría el
/// archivo al que apunta. Se arma con plomería de git, sin crear un enlace en
/// el disco, para que la prueba corra igual en cualquier sistema.
#[test]
fn los_enlaces_simbolicos_del_repositorio_se_omiten() {
    let repo = repo_de_ejemplo("simular-enlaces");
    let objeto = repo.git(["hash-object", "-w", "--", "espacios.md"]);
    repo.git([
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("120000,{objeto},enlace.md"),
    ]);
    repo.git(["commit", "--quiet", "-m", "un enlace"]);

    let plan = repo.agente().simular("main").expect("simular");
    assert!(plan
        .cambios()
        .iter()
        .all(|c| c.ruta().como_str() != "enlace.md"));
    let omitido = plan
        .omitidos()
        .iter()
        .find(|o| o.ruta == "enlace.md")
        .expect("omitido");
    assert_eq!(omitido.motivo, Omision::EnlaceSimbolico);
}
