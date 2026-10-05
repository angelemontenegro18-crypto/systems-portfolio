//! La interpretación de `robots.txt`, regla por regla de la RFC 9309.

use std::time::Duration;

use polite_crawler::Robots;

fn permite(robots: &str, token: &str, ruta: &str) -> bool {
    Robots::parsear(robots)
        .politica_para(token)
        .decidir(ruta)
        .permitida
}

#[test]
fn un_grupo_propio_gana_sobre_el_comodin() {
    let r = "User-agent: *\nDisallow: /\n\nUser-agent: lector\nDisallow: /privado\n";
    assert!(
        permite(r, "lector", "/publico"),
        "el grupo propio reemplaza al de `*`"
    );
    assert!(!permite(r, "lector", "/privado"));
    assert!(!permite(r, "otro", "/publico"), "los demás caen en `*`");
}

#[test]
fn el_agente_se_compara_sin_mayusculas_y_sin_version() {
    let r = "User-agent: LECTOR/2.0\nDisallow: /x\n";
    assert!(!permite(r, "lector", "/x"));
}

#[test]
fn varios_grupos_del_mismo_agente_se_combinan() {
    let r = "User-agent: lector\nDisallow: /a\n\nUser-agent: otro\nDisallow: /b\n\nUser-agent: lector\nDisallow: /c\n";
    assert!(!permite(r, "lector", "/a"));
    assert!(permite(r, "lector", "/b"));
    assert!(!permite(r, "lector", "/c"));
}

#[test]
fn varios_agentes_seguidos_comparten_un_grupo() {
    let r = "User-agent: uno\nUser-agent: lector\nDisallow: /compartido\n";
    assert!(!permite(r, "uno", "/compartido"));
    assert!(!permite(r, "lector", "/compartido"));
}

#[test]
fn gana_la_regla_mas_larga_y_ante_empate_allow() {
    let r = "User-agent: *\nDisallow: /docs/\nAllow: /docs/publicos/\n";
    assert!(!permite(r, "x", "/docs/internos/a"));
    assert!(
        permite(r, "x", "/docs/publicos/a"),
        "la regla más larga es la más específica"
    );

    let empate = "User-agent: *\nDisallow: /pagina\nAllow: /pagina\n";
    assert!(
        permite(empate, "x", "/pagina"),
        "ante un empate, la menos restrictiva"
    );
}

#[test]
fn comodines_y_ancla_de_fin() {
    let r = "User-agent: *\nDisallow: /*.pdf$\nDisallow: /tmp*/borrador\n";
    assert!(!permite(r, "x", "/informes/anual.pdf"));
    assert!(
        permite(r, "x", "/informes/anual.pdf?descarga=1"),
        "`$` ancla al final de la ruta"
    );
    assert!(!permite(r, "x", "/tmp123/borrador"));
    assert!(permite(r, "x", "/tmp/final"));
}

#[test]
fn la_decision_dice_que_regla_la_tomo() {
    let d = Robots::parsear("User-agent: *\nDisallow: /privado\n")
        .politica_para("x")
        .decidir("/privado/a");
    assert!(!d.permitida);
    assert_eq!(d.regla.as_deref(), Some("Disallow: /privado"));
}

#[test]
fn robots_txt_esta_siempre_permitido() {
    assert!(permite("User-agent: *\nDisallow: /\n", "x", "/robots.txt"));
}

#[test]
fn un_disallow_vacio_no_prohibe_nada() {
    assert!(permite("User-agent: *\nDisallow:\n", "x", "/lo-que-sea"));
}

#[test]
fn sin_grupo_aplicable_todo_esta_permitido() {
    assert!(permite("User-agent: otro\nDisallow: /\n", "lector", "/x"));
    assert!(permite("", "lector", "/x"));
}

#[test]
fn comentarios_mayusculas_en_las_claves_y_lineas_rotas_no_confunden() {
    let r = "# comentario\nUSER-AGENT: *  # al final\nDISALLOW: /a # otro\nesto no es una regla\nDisallow /sin-dos-puntos\n";
    assert!(!permite(r, "x", "/a"));
    assert!(permite(r, "x", "/sin-dos-puntos"));
}

#[test]
fn las_reglas_antes_de_cualquier_user_agent_se_ignoran() {
    assert!(permite(
        "Disallow: /\nUser-agent: *\nDisallow: /b\n",
        "x",
        "/a"
    ));
}

#[test]
fn utf8_y_porcentaje_son_la_misma_ruta() {
    assert!(!permite(
        "User-agent: *\nDisallow: /año\n",
        "x",
        "/a%C3%B1o/informe"
    ));
    assert!(!permite(
        "User-agent: *\nDisallow: /a%c3%b1o\n",
        "x",
        "/a%C3%B1o"
    ));
}

#[test]
fn crawl_delay_se_lee_y_se_queda_con_el_mayor() {
    let r = "User-agent: lector\nCrawl-delay: 2\n\nUser-agent: lector\nCrawl-delay: 5.5\nDisallow: /x\n";
    assert_eq!(
        Robots::parsear(r).politica_para("lector").crawl_delay,
        Some(Duration::from_millis(5500))
    );
    assert_eq!(
        Robots::parsear("User-agent: *\nCrawl-delay: -3\n")
            .politica_para("x")
            .crawl_delay,
        None
    );
}

#[test]
fn prohibir_todo_y_permitir_todo() {
    assert!(
        !Robots::prohibir_todo()
            .politica_para("x")
            .decidir("/a")
            .permitida
    );
    assert!(
        Robots::permitir_todo()
            .politica_para("x")
            .decidir("/a")
            .permitida
    );
}
