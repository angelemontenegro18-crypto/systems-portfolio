//! El vigía de servicios: interpretación de `systemctl show`, tiempo máximo y caché. El
//! tiempo es de mentira: ninguna prueba duerme.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ops_console::reloj::{Reloj, RelojDeMentira};
use ops_console::vigia::{
    esperar_con_limite, interpretar, unidad_valida, Ejecucion, EstadoServicio, Vigia,
    VigiaConCache, VigiaSystemctl, LIMITE, VIDA_CACHE,
};

#[test]
fn se_entiende_la_salida_de_systemctl_show() {
    let activo = interpretar(
        "LoadState=loaded\nActiveState=active\nSubState=running\nMainPID=4242\nNRestarts=3\n",
    );
    assert_eq!(
        activo,
        EstadoServicio::Activo {
            subestado: "running".into(),
            pid: Some(4242),
            reinicios: Some(3),
        }
    );
    assert_eq!(activo.clave(), "activo");
    assert_eq!(activo.detalle(), "running · pid 4242 · 3 reinicios");

    assert_eq!(
        interpretar("LoadState=loaded\nActiveState=active\nSubState=exited\nMainPID=0\n"),
        EstadoServicio::Activo {
            subestado: "exited".into(),
            pid: None,
            reinicios: None,
        }
    );
    assert_eq!(
        interpretar("LoadState=loaded\nActiveState=inactive\nSubState=dead\n").clave(),
        "inactivo"
    );
    assert_eq!(
        interpretar("LoadState=loaded\nActiveState=failed\nSubState=failed\n").clave(),
        "fallido"
    );
    assert_eq!(
        interpretar("LoadState=loaded\nActiveState=activating\nSubState=start\n"),
        EstadoServicio::EnTransicion {
            estado: "activating".into()
        }
    );
    assert_eq!(
        interpretar("LoadState=not-found\nActiveState=inactive\nSubState=dead\n"),
        EstadoServicio::NoEncontrado
    );
    assert_eq!(interpretar("").clave(), "error");
    assert_eq!(interpretar("ActiveState=quién sabe\n").clave(), "error");
}

#[test]
fn solo_se_consultan_nombres_de_unidad_razonables() {
    for valida in [
        "miniaturas-api.service",
        "getty@tty1.service",
        "a.timer",
        "x:y_z",
    ] {
        assert!(unidad_valida(valida), "{valida}");
    }
    let larga = "a".repeat(129);
    for invalida in [
        "",
        "-h",
        "--all",
        "a b",
        "../x",
        "x;y",
        "$(x)",
        "uno\ndos",
        "ñandú.service",
        larga.as_str(),
    ] {
        assert!(!unidad_valida(invalida), "{invalida:?}");
    }
    // Un nombre inválido ni siquiera llega a lanzar systemctl.
    let vigia = VigiaSystemctl::nueva(Arc::new(RelojDeMentira::nuevo()));
    assert_eq!(vigia.consultar("--all").clave(), "error");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn fuera_de_linux_el_vigia_dice_que_no_esta_disponible_y_por_que() {
    let vigia = VigiaSystemctl::nueva(Arc::new(RelojDeMentira::nuevo()));
    match vigia.consultar("miniaturas-api.service") {
        EstadoServicio::NoDisponible { motivo } => assert!(motivo.contains("Linux"), "{motivo}"),
        otro => panic!("se esperaba «no disponible»: {otro:?}"),
    }
}

/// Una consulta que no termina nunca y anota si la cortaron.
#[derive(Default)]
struct Colgada {
    miradas: usize,
    cortes: usize,
}

impl Ejecucion for Colgada {
    fn terminada(&mut self) -> Option<String> {
        self.miradas += 1;
        None
    }

    fn cortar(&mut self) {
        self.cortes += 1;
    }
}

/// Una consulta que termina después de `faltan` miradas.
struct Lenta {
    faltan: usize,
}

impl Ejecucion for Lenta {
    fn terminada(&mut self) -> Option<String> {
        if self.faltan == 0 {
            return Some("ActiveState=active\n".into());
        }
        self.faltan -= 1;
        None
    }

    fn cortar(&mut self) {
        panic!("no había que cortarla");
    }
}

#[test]
fn una_consulta_colgada_se_corta_a_los_tres_segundos_sin_dormir_de_verdad() {
    assert_eq!(LIMITE, Duration::from_secs(3));
    let reloj = RelojDeMentira::nuevo();
    let mut colgada = Colgada::default();
    let real = Instant::now();

    assert_eq!(esperar_con_limite(&mut colgada, LIMITE, &reloj), None);
    assert_eq!(colgada.cortes, 1, "se cortó una vez");
    assert_eq!(
        reloj.ahora_ms(),
        3000,
        "esperó exactamente el límite, en tiempo de mentira"
    );
    assert_eq!(
        colgada.miradas, 301,
        "una mirada cada 10 ms, más la del final"
    );
    assert!(
        real.elapsed() < Duration::from_secs(1),
        "no durmió de verdad"
    );
}

#[test]
fn una_consulta_que_termina_a_tiempo_devuelve_su_salida() {
    let reloj = RelojDeMentira::nuevo();
    let mut lenta = Lenta { faltan: 250 };
    assert_eq!(
        esperar_con_limite(&mut lenta, LIMITE, &reloj).as_deref(),
        Some("ActiveState=active\n")
    );
    assert_eq!(reloj.ahora_ms(), 2500);
}

/// Un vigía que cuenta sus consultas.
struct Contadora(Arc<AtomicUsize>);

impl Vigia for Contadora {
    fn consultar(&self, _unidad: &str) -> EstadoServicio {
        self.0.fetch_add(1, Ordering::SeqCst);
        EstadoServicio::Inactivo {
            subestado: "dead".into(),
        }
    }
}

#[test]
fn la_cache_guarda_cada_respuesta_diez_segundos_por_unidad() {
    assert_eq!(VIDA_CACHE, Duration::from_secs(10));
    let consultas = Arc::new(AtomicUsize::new(0));
    let reloj = Arc::new(RelojDeMentira::nuevo());
    let cache = VigiaConCache::nueva(Box::new(Contadora(Arc::clone(&consultas))), reloj.clone());

    let primera = cache.leer("a.service");
    assert!(!primera.de_cache);
    assert_eq!(consultas.load(Ordering::SeqCst), 1);

    reloj.avanzar(Duration::from_millis(9_999));
    let guardada = cache.leer("a.service");
    assert!(guardada.de_cache);
    assert_eq!(guardada.hace_ms, 9_999);
    assert_eq!(guardada.estado, primera.estado);
    assert_eq!(
        consultas.load(Ordering::SeqCst),
        1,
        "dentro de los 10 s no se consulta"
    );

    cache.leer("b.service");
    assert_eq!(
        consultas.load(Ordering::SeqCst),
        2,
        "otra unidad, otra consulta"
    );

    reloj.avanzar(Duration::from_millis(1));
    let vencida = cache.leer("a.service");
    assert!(!vencida.de_cache);
    assert_eq!(vencida.hace_ms, 0);
    assert_eq!(
        consultas.load(Ordering::SeqCst),
        3,
        "a los 10 s exactos se consulta de nuevo"
    );
}
