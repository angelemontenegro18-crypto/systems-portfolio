//! Nivel 2: preparar en un worktree aislado, sin tocar el árbol principal ni
//! la rama destino, y sin dejar restos si algo falla.

mod comun;

use comun::Repo;
use staged_agent::{Error, FinesDeLinea, Regla, PREFIJO_DE_RAMA};

/// Regla de prueba: todos los finales de línea a CRLF. Es idempotente.
struct ACrlf;

impl Regla for ACrlf {
    fn nombre(&self) -> &'static str {
        "a crlf"
    }

    fn aplicar(&self, texto: &str) -> String {
        FinesDeLinea.aplicar(texto).replace('\n', "\r\n")
    }
}

fn repo_de_ejemplo(nombre: &str) -> Repo {
    let repo = Repo::nuevo(nombre);
    repo.escribir("limpio.md", b"todo bien\n");
    repo.escribir("espacios.md", b"linea con espacios   \n");
    repo.escribir("crlf.txt", b"uno\r\ndos\r\n");
    repo.escribir("docs/sin_salto.md", b"sin salto final");
    repo.commit_todo("inicial");
    repo
}

#[test]
fn preparar_no_toca_el_arbol_principal_ni_la_rama_destino() {
    let repo = repo_de_ejemplo("preparar-intacto");
    let plan = repo.agente().simular("main").expect("simular");
    let arbol = repo.huella_del_arbol();
    let main = repo.punta("main");
    let head = std::fs::read(repo.raiz.join(".git").join("HEAD")).expect("HEAD");
    let indice = std::fs::read(repo.raiz.join(".git").join("index")).expect("index");

    repo.agente().preparar(&plan).expect("preparar");

    assert_eq!(repo.huella_del_arbol(), arbol, "cambió el árbol de trabajo");
    assert_eq!(repo.punta("main"), main, "se movió la rama destino");
    assert_eq!(
        std::fs::read(repo.raiz.join(".git").join("HEAD")).expect("HEAD"),
        head
    );
    assert_eq!(
        std::fs::read(repo.raiz.join(".git").join("index")).expect("index"),
        indice
    );
    assert_eq!(repo.estado(), "");
}

#[test]
fn la_rama_preparada_es_un_commit_con_exactamente_el_plan() {
    let repo = repo_de_ejemplo("preparar-contenido");
    let plan = repo.agente().simular("main").expect("simular");
    let preparado = repo.agente().preparar(&plan).expect("preparar");

    assert_eq!(
        preparado.rama(),
        format!("{PREFIJO_DE_RAMA}{}", plan.huella().corta())
    );
    assert_eq!(repo.punta(preparado.rama()), preparado.commit());
    assert_eq!(
        repo.git(["rev-parse", &format!("{}^@", preparado.commit())]),
        plan.base(),
        "un solo padre: la base"
    );

    let cambiados = repo.git(["diff", "--name-only", plan.base(), preparado.commit()]);
    let esperados: Vec<&str> = plan.cambios().iter().map(|c| c.ruta().como_str()).collect();
    assert_eq!(cambiados.lines().collect::<Vec<_>>(), esperados);
    for cambio in plan.cambios() {
        assert_eq!(
            repo.mostrar(preparado.commit(), cambio.ruta().como_str()),
            cambio.despues()
        );
    }
}

#[test]
fn preparar_no_deja_worktrees_registrados() {
    let repo = repo_de_ejemplo("preparar-sin-restos");
    let plan = repo.agente().simular("main").expect("simular");
    repo.agente().preparar(&plan).expect("preparar");
    assert_eq!(
        repo.cantidad_de_worktrees(),
        1,
        "solo debería quedar el principal"
    );
}

/// Sin identidad, `git commit` falla después de crear la rama y el worktree:
/// la limpieza tiene que quitar los dos.
#[test]
fn si_preparar_falla_a_medias_no_queda_worktree_ni_rama() {
    let repo = repo_de_ejemplo("preparar-falla");
    repo.git(["config", "--unset", "user.name"]);
    repo.git(["config", "--unset", "user.email"]);
    repo.git(["config", "user.useConfigOnly", "true"]);
    let plan = repo.agente().simular("main").expect("simular");
    let arbol = repo.huella_del_arbol();

    let resultado = repo.agente().preparar(&plan);
    assert!(matches!(resultado, Err(Error::Git(_))), "{resultado:?}");
    assert_eq!(repo.ramas(), ["refs/heads/main"], "quedó una rama huérfana");
    assert_eq!(
        repo.cantidad_de_worktrees(),
        1,
        "quedó un worktree huérfano"
    );
    assert_eq!(repo.huella_del_arbol(), arbol);
}

/// Una rama con el nombre que se iba a usar no es del agente: no se toca.
#[test]
fn una_rama_preexistente_con_ese_nombre_no_se_toca() {
    let repo = repo_de_ejemplo("preparar-rama-ajena");
    let plan = repo.agente().simular("main").expect("simular");
    let ajena = format!("{PREFIJO_DE_RAMA}{}", plan.huella().corta());
    repo.git(["branch", &ajena]);
    let punta = repo.punta(&ajena);

    assert!(matches!(
        repo.agente().preparar(&plan),
        Err(Error::RamaExistente(_))
    ));
    assert_eq!(repo.punta(&ajena), punta);
    assert_eq!(repo.cantidad_de_worktrees(), 1);
}

#[test]
fn un_plan_viejo_no_se_prepara() {
    let repo = repo_de_ejemplo("preparar-plan-viejo");
    let plan = repo.agente().simular("main").expect("simular");
    repo.escribir("otro.md", b"otro\n");
    repo.commit_todo("la destino avanza");

    assert!(matches!(
        repo.agente().preparar(&plan),
        Err(Error::DestinoMovido { .. })
    ));
    assert_eq!(repo.ramas(), ["refs/heads/main"]);
}

#[test]
fn un_plan_vacio_no_se_prepara() {
    let repo = Repo::nuevo("preparar-vacio");
    repo.escribir("limpio.md", b"todo bien\n");
    repo.commit_todo("inicial");
    let plan = repo.agente().simular("main").expect("simular");
    assert!(plan.esta_vacio());
    assert!(matches!(
        repo.agente().preparar(&plan),
        Err(Error::PlanVacio)
    ));
}

/// Un filtro de `git` cambia lo que se commitea: con `text eol=lf`, `git add`
/// vuelve a pasar a LF los CRLF que el plan escribió en `a.txt`, y el commit
/// solo lleva `b.md`. Ya no es el plan revisado: se rechaza y no queda nada.
#[test]
fn si_el_commit_no_reproduce_el_plan_se_deshace() {
    let repo = Repo::nuevo("preparar-inconsistente");
    repo.escribir("a.txt", b"uno\ndos\n");
    repo.escribir("b.md", b"uno\ndos\n");
    repo.commit_todo("inicial");
    // En `.git/info/attributes` y no en el árbol: así no entra en el plan.
    std::fs::write(
        repo.raiz.join(".git").join("info").join("attributes"),
        "*.txt text eol=lf\n",
    )
    .expect("escribir los atributos");
    let main = repo.punta("main");

    let agente = repo.agente().con_reglas(vec![Box::new(ACrlf)]);
    let plan = agente.simular("main").expect("simular");
    assert_eq!(plan.cambios().len(), 2);
    let resultado = agente.preparar(&plan);
    assert!(
        matches!(resultado, Err(Error::PreparacionInconsistente)),
        "{resultado:?}"
    );
    assert_eq!(
        repo.ramas(),
        ["refs/heads/main"],
        "quedó la rama del commit inconsistente"
    );
    assert_eq!(repo.cantidad_de_worktrees(), 1, "quedó un worktree");
    assert_eq!(repo.punta("main"), main);
}

/// Un filtro de checkout (`text eol=crlf`) hace que el worktree no tenga los
/// bytes que el plan revisó: no se escribe nada encima y no queda nada.
#[test]
fn si_el_worktree_no_tiene_lo_que_el_plan_espera_no_se_escribe() {
    let repo = Repo::nuevo("preparar-contenido-inesperado");
    repo.escribir("a.txt", b"espacios al final   \n");
    repo.commit_todo("inicial");
    std::fs::write(
        repo.raiz.join(".git").join("info").join("attributes"),
        "*.txt text eol=crlf\n",
    )
    .expect("escribir los atributos");

    let plan = repo.agente().simular("main").expect("simular");
    assert_eq!(plan.cambios().len(), 1);
    let resultado = repo.agente().preparar(&plan);
    assert!(
        matches!(resultado, Err(Error::ContenidoInesperado(_))),
        "{resultado:?}"
    );
    assert_eq!(repo.ramas(), ["refs/heads/main"]);
    assert_eq!(repo.cantidad_de_worktrees(), 1);
}
