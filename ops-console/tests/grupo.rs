//! El grupo de trabajadores. Casi todo con procesos de mentira, deterministas y sin dormir;
//! al final, **una** prueba de punta a punta con procesos reales (`trabajador-demo`).

mod comun;

use std::io::{BufRead, BufReader, Cursor, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use comun::{esperar_hasta, existe_proceso, matar_desde_afuera, Procesos};
use ops_console::grupo::{
    ErrorGrupo, Estado, FabricaDeProcesos, Grupo, Motivo, Politica, TOPE_DE_POSICIONES,
};
use ops_console::reloj::{RelojDeMentira, RelojReal};
use ops_console::trabajador::{atender, miniatura, responder};

const VIDA: Duration = Duration::from_secs(60);

fn politica(min: usize, max: usize) -> Politica {
    Politica::nueva(min, max, VIDA).expect("política válida")
}

fn grupo(min: usize, max: usize) -> (Grupo, Procesos, Arc<RelojDeMentira>) {
    let procesos = Procesos::default();
    let reloj = Arc::new(RelojDeMentira::nuevo());
    let grupo = Grupo::nuevo(politica(min, max), procesos.fabrica(), reloj.clone()).expect("grupo");
    (grupo, procesos, reloj)
}

fn ids(g: &mut Grupo) -> Vec<u32> {
    g.vista().posiciones.iter().map(|p| p.id).collect()
}

fn pids(g: &mut Grupo) -> Vec<u32> {
    g.vista().posiciones.iter().map(|p| p.pid).collect()
}

fn estados(g: &mut Grupo) -> Vec<Estado> {
    g.vista().posiciones.iter().map(|p| p.estado).collect()
}

// ─── Política ─────────────────────────────────────────────────────────────────────────────

#[test]
fn la_politica_exige_minimo_maximo_tope_y_un_segundo_de_vida() {
    for (min, max, vida) in [
        (0, 2, VIDA),
        (3, 2, VIDA),
        (1, TOPE_DE_POSICIONES + 1, VIDA),
        (1, 1, Duration::from_millis(999)),
    ] {
        assert!(
            matches!(
                Politica::nueva(min, max, vida),
                Err(ErrorGrupo::PoliticaInvalida(_))
            ),
            "{min} {max} {vida:?}"
        );
    }
    let p = Politica::nueva(1, TOPE_DE_POSICIONES, Duration::from_secs(1)).expect("válida");
    assert_eq!((p.min(), p.max()), (1, TOPE_DE_POSICIONES));
}

// ─── Arranque ─────────────────────────────────────────────────────────────────────────────

#[test]
fn un_grupo_nuevo_arranca_con_el_minimo_de_posiciones_vivas_y_numeradas() {
    let (mut g, procesos, _) = grupo(3, 6);
    let vista = g.vista();
    assert_eq!(ids(&mut g), [1, 2, 3]);
    assert!(vista
        .posiciones
        .iter()
        .all(|p| p.estado == Estado::Viva && p.generacion == 0));
    assert_eq!(pids(&mut g), procesos.lanzados());
}

#[test]
fn si_un_proceso_no_arranca_el_grupo_no_nace_ni_deja_procesos() {
    let procesos = Procesos::default();
    procesos.permitir_solo(2);
    let resultado = Grupo::nuevo(
        politica(3, 6),
        procesos.fabrica(),
        Arc::new(RelojDeMentira::nuevo()),
    );
    assert!(matches!(resultado, Err(ErrorGrupo::Lanzar(_))));
    assert_eq!(procesos.lanzados().len(), 2);
    assert!(
        procesos.sin_terminar().is_empty(),
        "los dos que sí arrancaron se terminaron"
    );
}

// ─── Reciclar ─────────────────────────────────────────────────────────────────────────────

#[test]
fn reciclar_conserva_la_posicion_y_el_id() {
    let (mut g, procesos, reloj) = grupo(3, 6);
    let antes = pids(&mut g);
    reloj.avanzar(Duration::from_secs(7));

    let accion = g.reciclar(2).expect("reciclar");
    assert_eq!(accion.posicion, 2);
    assert_eq!(accion.motivo, Motivo::Manual);
    assert_eq!(accion.pid_anterior, antes[1]);
    let nuevo = accion.resultado.expect("lanzado");

    let vista = g.vista();
    assert_eq!(ids(&mut g), [1, 2, 3], "mismo orden, mismos ids");
    assert_eq!(
        pids(&mut g),
        [antes[0], nuevo, antes[2]],
        "solo cambió el proceso de la 2"
    );
    let p = &vista.posiciones[1];
    assert_eq!(
        (p.estado, p.generacion, p.edad_ms, p.hace_ms),
        (Estado::Reciclada, 1, 0, 0)
    );
    assert_eq!(vista.posiciones[0].edad_ms, 7000, "las demás no se tocaron");
    assert_eq!(procesos.terminados(), [antes[1]]);
}

#[test]
fn reciclar_lanza_el_reemplazo_antes_de_terminar_al_viejo() {
    let (mut g, procesos, _) = grupo(1, 2);
    let viejo = pids(&mut g)[0];
    let nuevo = g.reciclar(1).expect("reciclar").resultado.expect("lanzado");
    assert_eq!(
        procesos.eventos(),
        [('L', viejo), ('L', nuevo), ('T', viejo)]
    );
}

#[test]
fn si_el_reemplazo_no_arranca_el_viejo_sigue_en_su_lugar() {
    let (mut g, procesos, _) = grupo(2, 4);
    let antes = pids(&mut g);
    procesos.fallar(1);
    assert!(matches!(g.reciclar(1), Err(ErrorGrupo::Lanzar(_))));
    assert_eq!(pids(&mut g), antes);
    assert_eq!(estados(&mut g), [Estado::Viva, Estado::Viva]);
    assert!(procesos.terminados().is_empty(), "nadie terminó al viejo");
}

#[test]
fn reciclar_una_posicion_que_no_existe_es_un_error() {
    let (mut g, _, _) = grupo(2, 4);
    assert!(matches!(
        g.reciclar(99),
        Err(ErrorGrupo::PosicionDesconocida(99))
    ));
    assert!(matches!(
        g.reciclar(0),
        Err(ErrorGrupo::PosicionDesconocida(0))
    ));
}

// ─── Escalar ──────────────────────────────────────────────────────────────────────────────

#[test]
fn escalar_respeta_el_minimo_y_el_maximo() {
    let (mut g, procesos, _) = grupo(2, 5);
    for cantidad in [0, 1, 6, 100] {
        match g.escalar(cantidad) {
            Err(ErrorGrupo::FueraDePolitica {
                cantidad: c,
                min,
                max,
            }) => {
                assert_eq!((c, min, max), (cantidad, 2, 5));
            }
            otro => panic!("escalar a {cantidad} debía fallar: {otro:?}"),
        }
        assert_eq!(
            g.cantidad(),
            2,
            "un pedido fuera de la política no cambia nada"
        );
    }
    assert_eq!(procesos.lanzados().len(), 2);

    let subida = g.escalar(5).expect("subir");
    assert_eq!((subida.antes, subida.despues), (2, 5));
    assert_eq!(ids(&mut g), [1, 2, 3, 4, 5]);

    let mut quitados: Vec<u32> = pids(&mut g)[2..].to_vec();
    let bajada = g.escalar(2).expect("bajar");
    assert_eq!((bajada.antes, bajada.despues), (5, 2));
    assert_eq!(ids(&mut g), [1, 2], "al bajar salen las más nuevas");
    quitados.reverse();
    assert_eq!(
        procesos.terminados(),
        quitados,
        "de la más nueva a la más vieja"
    );
}

#[test]
fn cambiar_la_politica_ajusta_el_grupo_o_no_cambia_nada() {
    let (mut g, _, _) = grupo(2, 8);
    g.escalar(6).expect("escalar");
    assert_eq!(g.cambiar_politica(1, 4).expect("bajar").despues, 4);
    assert_eq!(g.cambiar_politica(5, 8).expect("subir").despues, 5);
    assert!(matches!(
        g.cambiar_politica(3, 2),
        Err(ErrorGrupo::PoliticaInvalida(_))
    ));
    assert_eq!(
        (g.politica().min(), g.politica().max(), g.cantidad()),
        (5, 8, 5)
    );
}

// ─── Tick ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn el_tick_reemplaza_lo_que_murio_en_su_lugar_y_dice_que_hizo() {
    let (mut g, procesos, reloj) = grupo(3, 6);
    let antes = pids(&mut g);
    procesos.matar(antes[1]);

    let vista = g.vista();
    assert_eq!(vista.posiciones[1].estado, Estado::Muerta);
    reloj.avanzar(Duration::from_secs(5));
    assert_eq!(
        g.vista().posiciones[1].hace_ms,
        5000,
        "muerta desde hace 5 s"
    );

    let acciones = g.tick();
    assert_eq!(acciones.len(), 1);
    let a = &acciones[0];
    assert_eq!(
        (a.posicion, a.motivo, a.pid_anterior),
        (2, Motivo::Murio, antes[1])
    );
    let nuevo = a.resultado.clone().expect("lanzado");
    assert_eq!(ids(&mut g), [1, 2, 3]);
    assert_eq!(pids(&mut g), [antes[0], nuevo, antes[2]]);
    assert_eq!(
        estados(&mut g),
        [Estado::Viva, Estado::Reciclada, Estado::Viva]
    );
    assert!(
        procesos.terminados().contains(&antes[1]),
        "al muerto también se lo espera"
    );

    assert!(
        g.tick().is_empty(),
        "un segundo tick no tiene nada que hacer"
    );
}

#[test]
fn el_tick_recicla_lo_que_cumplio_su_tiempo_de_vida() {
    let (mut g, _, reloj) = grupo(3, 6);
    reloj.avanzar(VIDA - Duration::from_millis(1));
    assert!(g.tick().is_empty());
    reloj.avanzar(Duration::from_millis(1));
    let acciones = g.tick();
    assert_eq!(acciones.len(), 3);
    assert!(acciones
        .iter()
        .all(|a| a.motivo == Motivo::VidaCumplida && a.resultado.is_ok()));
    assert!(g
        .vista()
        .posiciones
        .iter()
        .all(|p| p.edad_ms == 0 && p.generacion == 1));
}

#[test]
fn un_reemplazo_fallido_queda_anotado_y_el_siguiente_tick_lo_reintenta() {
    let (mut g, procesos, _) = grupo(2, 4);
    let caido = pids(&mut g)[0];
    procesos.matar(caido);
    procesos.fallar(1);

    let acciones = g.tick();
    assert_eq!(acciones.len(), 1);
    assert!(acciones[0].resultado.is_err());
    assert_eq!(estados(&mut g)[0], Estado::Muerta);

    let acciones = g.tick();
    assert!(acciones[0].resultado.is_ok());
    assert_eq!(estados(&mut g)[0], Estado::Reciclada);
}

// ─── Ningún huérfano ──────────────────────────────────────────────────────────────────────

#[test]
fn soltar_el_grupo_termina_todos_sus_procesos() {
    let (mut g, procesos, _) = grupo(3, 8);
    g.escalar(6).expect("escalar");
    g.reciclar(2).expect("reciclar");
    procesos.matar(pids(&mut g)[4]);
    drop(g);
    assert_eq!(procesos.lanzados().len(), 7);
    assert!(
        procesos.sin_terminar().is_empty(),
        "{:?}",
        procesos.sin_terminar()
    );
}

#[test]
fn ni_un_panico_deja_procesos_sin_terminar() {
    let procesos = Procesos::default();
    let resultado = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut g = Grupo::nuevo(
            politica(3, 6),
            procesos.fabrica(),
            Arc::new(RelojDeMentira::nuevo()),
        )
        .expect("grupo");
        g.escalar(5).expect("escalar");
        panic!("algo salió mal con el grupo en la mano");
    }));
    assert!(resultado.is_err());
    assert_eq!(procesos.lanzados().len(), 5);
    assert!(
        procesos.sin_terminar().is_empty(),
        "{:?}",
        procesos.sin_terminar()
    );
}

// ─── El trabajador ────────────────────────────────────────────────────────────────────────

#[test]
fn el_trabajador_calcula_miniaturas_sin_deformar_ni_agrandar() {
    assert_eq!(miniatura(4000, 3000, 400), Some((400, 300)));
    assert_eq!(miniatura(3000, 4000, 400), Some((300, 400)));
    assert_eq!(miniatura(1000, 1000, 256), Some((256, 256)));
    assert_eq!(miniatura(100, 50, 256), Some((100, 50)));
    assert_eq!(miniatura(10_000, 1, 100), Some((100, 1)), "nunca 0 de alto");
    assert_eq!(miniatura(0, 10, 10), None);
    assert_eq!(miniatura(u32::MAX, u32::MAX - 1, 1000), Some((1000, 1000)));

    assert_eq!(responder("4000 3000 400"), "ok 400 300");
    assert!(responder("4000 x 400").starts_with("error"));
    assert!(responder("0 10 10").starts_with("error"));
    assert!(responder("").starts_with("error"));

    let mut salida = Vec::new();
    let atendidas = atender(Cursor::new("1920 1080 256\n1 2\n"), &mut salida).expect("atender");
    assert_eq!(atendidas, 2);
    let texto = String::from_utf8(salida).expect("UTF-8");
    assert_eq!(
        texto.lines().collect::<Vec<_>>(),
        ["ok 256 144", "error se espera «ANCHO ALTO LADO»"]
    );
}

// ─── De punta a punta, con procesos reales ────────────────────────────────────────────────

#[test]
fn de_punta_a_punta_con_procesos_reales() {
    let programa = env!("CARGO_BIN_EXE_trabajador-demo");

    // El trabajador habla su protocolo y termina al cerrarse su entrada.
    let mut solo = Command::new(programa)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("lanzar el trabajador");
    let mut entrada = solo.stdin.take().expect("entrada");
    let mut salida = BufReader::new(solo.stdout.take().expect("salida"));
    writeln!(entrada, "4000 3000 400").expect("escribir");
    let mut linea = String::new();
    salida.read_line(&mut linea).expect("leer");
    assert_eq!(linea.trim_end(), "ok 400 300");
    drop(entrada);
    assert!(
        solo.wait().expect("esperar").success(),
        "terminó solo al cerrarse su entrada"
    );

    // El grupo, con procesos de verdad.
    let fabrica = FabricaDeProcesos::nueva(programa, Vec::new());
    let mut g = Grupo::nuevo(
        Politica::nueva(2, 4, Duration::from_secs(3600)).expect("política"),
        Box::new(fabrica),
        Arc::new(RelojReal::nuevo()),
    )
    .expect("grupo");
    let iniciales = pids(&mut g);
    assert!(
        iniciales.iter().all(|&pid| existe_proceso(pid)),
        "{iniciales:?}"
    );

    let reciclado = g.reciclar(1).expect("reciclar");
    let nuevo = reciclado.resultado.clone().expect("lanzado");
    assert!(
        !existe_proceso(reciclado.pid_anterior),
        "el viejo se terminó y se esperó"
    );
    assert!(existe_proceso(nuevo));
    assert_eq!(ids(&mut g), [1, 2]);

    // Una caída de verdad: el tick la ve y la reemplaza en su lugar.
    let victima = pids(&mut g)[1];
    matar_desde_afuera(victima);
    assert!(
        esperar_hasta(Duration::from_secs(5), || g.vista().posiciones[1].estado
            == Estado::Muerta),
        "no se vio la caída"
    );
    let acciones = g.tick();
    assert_eq!(acciones.len(), 1);
    assert_eq!(
        (acciones[0].posicion, acciones[0].motivo),
        (2, Motivo::Murio)
    );
    assert!(!existe_proceso(victima));
    assert_eq!(ids(&mut g), [1, 2]);

    g.escalar(4).expect("escalar");
    let todos = pids(&mut g);
    assert_eq!(todos.len(), 4);
    assert!(todos.iter().all(|&pid| existe_proceso(pid)));

    // Un pánico con el grupo en la mano: al desenrollar, se terminan y esperan todos.
    let hilo = thread::spawn(move || {
        let _grupo = g;
        panic!("pánico con procesos reales en el grupo");
    });
    assert!(hilo.join().is_err());
    let sobrevivientes: Vec<u32> = todos
        .into_iter()
        .filter(|&pid| existe_proceso(pid))
        .collect();
    assert!(
        sobrevivientes.is_empty(),
        "quedaron vivos: {sobrevivientes:?}"
    );
}
