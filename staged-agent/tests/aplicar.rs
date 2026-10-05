//! Nivel 3: aplicar lleva a la rama destino solo el plan exacto que se revisó,
//! y cada rechazo deja todo como estaba.

mod comun;

use comun::Repo;
use staged_agent::{Confirmacion, Error, FinesDeLinea, Plan, Preparado};

fn repo_preparado(nombre: &str) -> (Repo, Plan, Preparado) {
    let repo = Repo::nuevo(nombre);
    repo.escribir("limpio.md", b"todo bien\n");
    repo.escribir("espacios.md", b"linea con espacios   \n");
    repo.escribir("crlf.txt", b"uno\r\ndos\r\n");
    repo.commit_todo("inicial");
    let plan = repo.agente().simular("main").expect("simular");
    let preparado = repo.agente().preparar(&plan).expect("preparar");
    (repo, plan, preparado)
}

fn confirmar(plan: &Plan) -> Confirmacion {
    Confirmacion::escrita(plan.confirmacion_esperada())
}

/// Lo que no puede cambiar cuando aplicar se niega.
struct Intacto {
    arbol: String,
    main: String,
}

impl Intacto {
    fn de(repo: &Repo) -> Intacto {
        Intacto {
            arbol: repo.huella_del_arbol(),
            main: repo.punta("main"),
        }
    }

    fn comprobar(&self, repo: &Repo) {
        assert_eq!(
            repo.huella_del_arbol(),
            self.arbol,
            "un rechazo cambió el árbol"
        );
        assert_eq!(
            repo.punta("main"),
            self.main,
            "un rechazo movió la rama destino"
        );
    }
}

#[test]
fn aplicar_lleva_el_plan_exacto_a_la_rama_destino() {
    let (repo, plan, preparado) = repo_preparado("aplicar-ok");
    let aplicado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan))
        .expect("aplicar");

    assert_eq!(aplicado.commit, preparado.commit());
    assert_eq!(aplicado.archivos, plan.cambios().len());
    assert_eq!(repo.punta("main"), preparado.commit());
    assert_eq!(repo.leer("espacios.md"), b"linea con espacios\n");
    assert_eq!(repo.leer("crlf.txt"), b"uno\ndos\n");
    assert_eq!(repo.leer("limpio.md"), b"todo bien\n");
    assert_eq!(repo.estado(), "");
    assert_eq!(
        repo.ramas(),
        ["refs/heads/main"],
        "la rama preparada ya no hace falta"
    );
}

/// Alguien cambia el contenido de la rama después de prepararla (aquí, con
/// `commit --amend` en un worktree aparte). La huella recalculada ya no
/// coincide.
#[test]
fn aplicar_rechaza_un_plan_alterado_despues_de_prepararlo() {
    let (repo, plan, preparado) = repo_preparado("aplicar-alterado");
    let worktree = repo.directorio_aparte("alteracion").join("wt");
    repo.git([
        std::ffi::OsStr::new("worktree"),
        "add".as_ref(),
        worktree.as_os_str(),
        preparado.rama().as_ref(),
    ]);
    std::fs::write(worktree.join("espacios.md"), "contenido que nadie revisó\n").expect("alterar");
    repo.git_en(
        &worktree,
        ["commit", "--quiet", "--amend", "-a", "-m", "alterado"],
    );
    repo.git([
        std::ffi::OsStr::new("worktree"),
        "remove".as_ref(),
        "--force".as_ref(),
        worktree.as_os_str(),
    ]);

    let intacto = Intacto::de(&repo);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::PlanAlterado)),
        "{resultado:?}"
    );
    intacto.comprobar(&repo);
}

/// Un commit agregado encima de la rama preparada tampoco pasa.
#[test]
fn aplicar_rechaza_commits_agregados_a_la_rama_preparada() {
    let (repo, plan, preparado) = repo_preparado("aplicar-agregado");
    let worktree = repo.directorio_aparte("agregado").join("wt");
    repo.git([
        std::ffi::OsStr::new("worktree"),
        "add".as_ref(),
        worktree.as_os_str(),
        preparado.rama().as_ref(),
    ]);
    std::fs::write(worktree.join("limpio.md"), b"otra cosa\n").expect("escribir");
    repo.git_en(&worktree, ["commit", "--quiet", "-a", "-m", "encima"]);
    repo.git([
        std::ffi::OsStr::new("worktree"),
        "remove".as_ref(),
        "--force".as_ref(),
        worktree.as_os_str(),
    ]);

    let intacto = Intacto::de(&repo);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::PlanAlterado)),
        "{resultado:?}"
    );
    intacto.comprobar(&repo);
}

/// La huella revisada es la de otro plan: no se aplica, aunque la confirmación
/// corresponda a esa otra huella.
#[test]
fn aplicar_rechaza_una_huella_que_no_es_la_preparada() {
    let (repo, _plan, preparado) = repo_preparado("aplicar-otra-huella");
    let otro = repo
        .agente()
        .con_reglas(vec![Box::new(FinesDeLinea)])
        .simular("main")
        .expect("simular otro plan");
    let intacto = Intacto::de(&repo);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &otro.huella(), &confirmar(&otro));
    assert!(
        matches!(resultado, Err(Error::PlanAlterado)),
        "{resultado:?}"
    );
    intacto.comprobar(&repo);
}

#[test]
fn aplicar_rechaza_un_arbol_sucio() {
    let (repo, plan, preparado) = repo_preparado("aplicar-sucio");
    repo.escribir("limpio.md", b"modificado sin commitear\n");
    let intacto = Intacto::de(&repo);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::ArbolSucio(_))),
        "{resultado:?}"
    );
    intacto.comprobar(&repo);

    // Un archivo nuevo sin seguimiento también cuenta.
    repo.git(["checkout", "--quiet", "--", "limpio.md"]);
    repo.escribir("nuevo.txt", b"sin seguimiento\n");
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::ArbolSucio(_))),
        "{resultado:?}"
    );
}

#[test]
fn aplicar_rechaza_sin_confirmacion_explicita_de_este_plan() {
    let (repo, plan, preparado) = repo_preparado("aplicar-confirmacion");
    let intacto = Intacto::de(&repo);
    let completa = format!("aplicar {}", plan.huella());
    for texto in [
        "",
        "sí",
        "aplicar",
        "aplicar 000000000000",
        completa.as_str(),
    ] {
        let resultado =
            repo.agente()
                .aplicar(&preparado, &plan.huella(), &Confirmacion::escrita(texto));
        assert!(
            matches!(resultado, Err(Error::SinConfirmacion)),
            "{texto:?}: {resultado:?}"
        );
    }
    intacto.comprobar(&repo);
}

#[test]
fn aplicar_rechaza_si_la_rama_destino_se_movio() {
    let (repo, plan, preparado) = repo_preparado("aplicar-destino-movido");
    repo.escribir("otro.md", b"otro\n");
    repo.commit_todo("la destino avanza");
    let intacto = Intacto::de(&repo);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::DestinoMovido { .. })),
        "{resultado:?}"
    );
    intacto.comprobar(&repo);
}

#[test]
fn aplicar_rechaza_si_la_rama_en_uso_es_otra() {
    let (repo, plan, preparado) = repo_preparado("aplicar-otra-rama");
    repo.git(["switch", "--quiet", "-c", "otra"]);
    let resultado = repo
        .agente()
        .aplicar(&preparado, &plan.huella(), &confirmar(&plan));
    assert!(
        matches!(resultado, Err(Error::OtraRamaEnUso { .. })),
        "{resultado:?}"
    );
    assert_eq!(repo.punta("main"), plan.base());
}

/// Otra ejecución, que solo conoce el nombre de la rama, la destino y la
/// huella, puede aplicar: todo se recalcula desde el repositorio.
#[test]
fn aplicar_desde_otra_ejecucion_con_solo_la_rama_y_la_huella() {
    let (repo, plan, preparado) = repo_preparado("aplicar-desde");
    let reconstruido = Preparado::desde(preparado.rama(), "main", plan.huella());
    repo.agente()
        .aplicar(&reconstruido, &plan.huella(), &confirmar(&plan))
        .expect("aplicar");
    assert_eq!(repo.punta("main"), preparado.commit());
}

/// Los nombres que llegan por `Preparado::desde` se validan antes de llegar a
/// `git`, igual que en `simular`.
#[test]
fn aplicar_rechaza_nombres_de_rama_peligrosos() {
    let (repo, plan, _preparado) = repo_preparado("aplicar-nombres");
    let intacto = Intacto::de(&repo);
    for (rama, destino) in [
        ("--force", "main"),
        ("staged-agent/../main", "main"),
        ("staged-agent/x", "-x"),
    ] {
        let falso = Preparado::desde(rama, destino, plan.huella());
        let resultado = repo
            .agente()
            .aplicar(&falso, &plan.huella(), &confirmar(&plan));
        assert!(
            matches!(resultado, Err(Error::NombreDeRamaInvalido(_))),
            "{rama:?}, {destino:?}: {resultado:?}"
        );
    }
    intacto.comprobar(&repo);
}
