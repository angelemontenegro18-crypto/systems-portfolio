//! Sesiones efímeras y pool, contra el trabajador de demostración real.

use std::time::{Duration, Instant};

use session_isolation::demo::{ConfigCompartida, Peticion, Respuesta, Tarea};
use session_isolation::{
    ejecutar, ErrorIpc, ErrorPool, Evento, Juez, Limites, Medicion, Pool, Publicador, Receta,
    Umbrales,
};

fn trabajador(args: &[&str]) -> Receta {
    Receta::nueva(env!("CARGO_BIN_EXE_trabajador-demo"), args)
}

fn peticion(tarea: Tarea) -> Peticion {
    Peticion {
        generacion: 0,
        config: ConfigCompartida {
            max_elementos: 8,
            factor: 3,
        },
        tarea,
    }
}

fn limites(ms: u64) -> Limites {
    Limites {
        tiempo: Duration::from_millis(ms),
        max_bytes_salida: 64 * 1024,
    }
}

/// En Linux, un proceso esperado desaparece de `/proc`; un zombi seguiría ahí.
#[cfg(target_os = "linux")]
fn fue_cosechado(pid: u32) -> bool {
    !std::path::Path::new(&format!("/proc/{pid}")).exists()
}

// ─── Sesiones efímeras ───────────────────────────────────────────────────

#[test]
fn una_sesion_trabaja_sobre_la_instantanea_que_recibio() {
    let config = Publicador::nuevo(&ConfigCompartida {
        max_elementos: 8,
        factor: 2,
    });
    config.publicar(&ConfigCompartida {
        max_elementos: 8,
        factor: 10,
    });

    let foto = config.leer();
    let p = Peticion {
        generacion: foto.generacion,
        config: foto.valor,
        tarea: Tarea::Escalar(vec![1, 2, 3]),
    };

    // Publicar después de sacar la foto no afecta a la sesión: tiene su copia.
    config.publicar(&ConfigCompartida {
        max_elementos: 8,
        factor: 1000,
    });

    let r: Respuesta = ejecutar(&trabajador(&[]), &p, &limites(5_000)).expect("sesión");
    assert_eq!(
        r,
        Respuesta {
            generacion: 1,
            valores: vec![10, 20, 30]
        }
    );
}

#[test]
fn un_proceso_colgado_se_termina_y_se_cosecha() {
    let inicio = Instant::now();
    let e = ejecutar::<_, Respuesta>(
        &trabajador(&[]),
        &peticion(Tarea::Dormir { ms: 30_000 }),
        &limites(300),
    )
    .expect_err("se tiene que vencer el plazo");
    assert!(
        inicio.elapsed() < Duration::from_secs(5),
        "tardó {:?}",
        inicio.elapsed()
    );
    let ErrorIpc::Timeout { pid, .. } = e else {
        panic!("{e:?}")
    };
    #[cfg(target_os = "linux")]
    assert!(fue_cosechado(pid), "el proceso {pid} quedó zombi");
    let _ = pid;
}

#[test]
fn una_salida_desbordada_se_corta() {
    let e = ejecutar::<_, Respuesta>(
        &trabajador(&[]),
        &peticion(Tarea::Inundar { bytes: 10 << 20 }),
        &limites(5_000),
    )
    .expect_err("la salida pasa el tope");
    assert!(
        matches!(e, ErrorIpc::SalidaExcedida { limite: 65_536 }),
        "{e:?}"
    );
}

#[test]
fn un_proceso_que_falla_reporta_codigo_y_stderr() {
    let e = ejecutar::<_, Respuesta>(
        &trabajador(&[]),
        &peticion(Tarea::Fallar { codigo: 7 }),
        &limites(5_000),
    )
    .expect_err("el proceso falla");
    let ErrorIpc::Termino { codigo, stderr } = e else {
        panic!("{e:?}")
    };
    assert_eq!(codigo, Some(7));
    assert!(stderr.contains("falla pedida"), "{stderr}");
}

#[test]
fn el_limite_de_la_configuracion_lo_aplica_el_proceso() {
    let p = peticion(Tarea::Escalar((0..20).collect()));
    let e = ejecutar::<_, Respuesta>(&trabajador(&[]), &p, &limites(5_000))
        .expect_err("demasiados elementos");
    assert!(
        matches!(
            e,
            ErrorIpc::Termino {
                codigo: Some(3),
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn un_programa_inexistente_es_un_error_de_lanzamiento() {
    let receta = Receta::nueva("/no/existe/este-programa", &[]);
    let e = ejecutar::<_, Respuesta>(&receta, &peticion(Tarea::Escalar(vec![])), &limites(1_000))
        .expect_err("");
    assert!(matches!(e, ErrorIpc::Lanzar(_)), "{e:?}");
}

// ─── Pool ────────────────────────────────────────────────────────────────

#[test]
fn reemplazar_cambia_el_proceso_y_conserva_la_identidad_del_slot() {
    let mut pool = Pool::nuevo(2, 1);
    let id = pool
        .lanzar("ingesta", trabajador(&["residente"]))
        .expect("lanzar");
    let antes = pool.vista().remove(0);

    let evento = pool.reemplazar(id, "prueba").expect("reemplazar");
    let despues = pool.vista().remove(0);

    let Evento::Reemplazado {
        pid_anterior,
        pid_nuevo,
        ..
    } = evento
    else {
        panic!("{evento:?}")
    };
    assert_eq!(pid_anterior, antes.pid);
    assert_eq!(pid_nuevo, despues.pid);
    assert_ne!(antes.pid, despues.pid, "el proceso es otro");

    assert_eq!(despues.meta.id, antes.meta.id);
    assert_eq!(despues.meta.etiqueta, "ingesta");
    assert_eq!(
        despues.meta.creado, antes.meta.creado,
        "el slot es el mismo"
    );
    assert_eq!(despues.meta.reemplazos, 1);
    assert!(despues.proceso_desde > antes.proceso_desde);

    #[cfg(target_os = "linux")]
    assert!(fue_cosechado(antes.pid), "el proceso viejo quedó zombi");
}

#[test]
fn sin_margen_el_reemplazo_se_rechaza_y_la_victima_sigue() {
    // Pool lleno y sin margen: no hay lugar para tener dos procesos a la vez.
    // Si el pool matara primero y lanzara después, esto funcionaría — que se
    // rechace es la prueba de que el nuevo arranca antes que termine el viejo.
    let mut pool = Pool::nuevo(1, 0);
    let id = pool
        .lanzar("único", trabajador(&["residente"]))
        .expect("lanzar");
    let pid = pool.vista()[0].pid;

    let e = pool.reemplazar(id, "prueba").expect_err("sin margen");
    assert!(
        matches!(
            e,
            ErrorPool::SinMargen {
                procesos: 2,
                tope: 1
            }
        ),
        "{e:?}"
    );
    assert_eq!(pool.vista()[0].pid, pid);
    assert!(pool.caidos().is_empty(), "la víctima sigue viva");
}

#[test]
fn si_el_reemplazo_no_arranca_el_slot_queda_intacto() {
    let mut pool = Pool::nuevo(1, 1);
    let receta_buena = trabajador(&["residente"]);
    let id = pool
        .lanzar("servicio", receta_buena.clone())
        .expect("lanzar");
    let antes = pool.vista().remove(0);

    let mala = Receta::nueva("/no/existe/version-nueva", &[]);
    let e = pool
        .reemplazar_con_receta(id, Some(mala), "actualización")
        .expect_err("no arranca");
    assert!(matches!(e, ErrorPool::Lanzar(_)), "{e:?}");

    let despues = pool.vista().remove(0);
    assert_eq!(
        despues.pid, antes.pid,
        "el proceso anterior sigue atendiendo"
    );
    assert_eq!(despues.meta.receta, receta_buena, "y conserva su receta");
    assert_eq!(despues.meta.reemplazos, 0);
    assert!(pool.caidos().is_empty());
}

#[test]
fn una_actualizacion_continua_cambia_la_receta_slot_por_slot() {
    let mut pool = Pool::nuevo(2, 1);
    let a = pool.lanzar("a", trabajador(&["residente"])).expect("a");
    let b = pool.lanzar("b", trabajador(&["residente"])).expect("b");
    let v2 = Receta::nueva(
        env!("CARGO_BIN_EXE_trabajador-demo"),
        &["residente", "--version-2"],
    );

    pool.reemplazar_con_receta(a, Some(v2.clone()), "v2")
        .expect("a → v2");
    let vista = pool.vista();
    assert_eq!(vista[0].meta.receta, v2);
    assert_ne!(vista[1].meta.receta, v2, "b todavía no se actualizó");

    pool.reemplazar_con_receta(b, Some(v2.clone()), "v2")
        .expect("b → v2");
    assert!(pool.vista().iter().all(|s| s.meta.receta == v2));
}

#[test]
fn el_ciclo_de_salud_reemplaza_solo_al_slot_enfermo() {
    let mut pool = Pool::nuevo(3, 1);
    let sano = pool
        .lanzar("sano", trabajador(&["residente"]))
        .expect("sano");
    let enfermo = pool
        .lanzar("enfermo", trabajador(&["residente"]))
        .expect("enfermo");
    let pid_enfermo = pool.vista()[1].pid;

    let mut juez = Juez::nuevo(Umbrales {
        persistencia: 2,
        ..Umbrales::default()
    });
    // Mediciones inyectadas: el enfermo usa 1 GiB; el sano, 10 MiB.
    let medir = |pid: u32| Medicion {
        memoria_bytes: Some(if pid == pid_enfermo {
            1 << 30
        } else {
            10 << 20
        }),
        latencia: None,
    };

    assert!(
        pool.ciclo_de_salud(&mut juez, medir).is_empty(),
        "un pico no alcanza"
    );
    let eventos = pool.ciclo_de_salud(&mut juez, medir);
    assert_eq!(eventos.len(), 1, "{eventos:?}");
    assert!(matches!(&eventos[0], Evento::Reemplazado { slot, .. } if *slot == enfermo));

    let vista = pool.vista();
    assert_eq!(
        vista
            .iter()
            .find(|s| s.meta.id == sano)
            .map(|s| s.meta.reemplazos),
        Some(0)
    );
    assert_eq!(
        vista
            .iter()
            .find(|s| s.meta.id == enfermo)
            .map(|s| s.meta.reemplazos),
        Some(1)
    );
}

#[test]
fn un_proceso_caido_se_reemplaza_en_el_siguiente_ciclo() {
    let mut pool = Pool::nuevo(1, 1);
    let id = pool
        .lanzar("frágil", trabajador(&["residente"]))
        .expect("lanzar");
    let pid = pool.vista()[0].pid;

    // Se simula una caída matando el proceso desde afuera del pool.
    #[cfg(unix)]
    {
        let estado = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()
            .expect("kill");
        assert!(estado.success());
    }
    #[cfg(windows)]
    {
        let estado = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status()
            .expect("taskkill");
        assert!(estado.success());
    }

    let limite = Instant::now() + Duration::from_secs(5);
    while pool.caidos().is_empty() {
        assert!(Instant::now() < limite, "no se detectó la caída");
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut juez = Juez::default();
    let eventos = pool.ciclo_de_salud(&mut juez, |_| Medicion::default());
    assert!(
        matches!(&eventos[0], Evento::Reemplazado { slot, pid_anterior, .. } if *slot == id && *pid_anterior == pid)
    );
    assert!(pool.caidos().is_empty(), "el reemplazo está vivo");
}

#[test]
fn sin_cupo_no_se_lanza_otro_slot() {
    let mut pool = Pool::nuevo(1, 5);
    pool.lanzar("uno", trabajador(&["residente"])).expect("uno");
    assert!(matches!(
        pool.lanzar("dos", trabajador(&["residente"])),
        Err(ErrorPool::SinCupo { max: 1 })
    ));
}

#[test]
#[cfg(target_os = "linux")]
fn soltar_el_pool_termina_y_cosecha_todos_sus_procesos() {
    let mut pool = Pool::nuevo(3, 0);
    for i in 0..3 {
        pool.lanzar(format!("s{i}"), trabajador(&["residente"]))
            .expect("lanzar");
    }
    let pids: Vec<u32> = pool.vista().iter().map(|s| s.pid).collect();
    drop(pool);
    for pid in pids {
        assert!(fue_cosechado(pid), "el proceso {pid} sobrevivió al pool");
    }
}
