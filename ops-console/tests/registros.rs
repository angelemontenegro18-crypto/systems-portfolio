//! Registros sin secretos: la redacción (muchas formas, mayúsculas, saltos de línea,
//! idempotencia) y la lectura de la cola con tope de bytes. También la presencia de variables.

mod comun;

use std::io::ErrorKind;

use comun::Carpeta;
use ops_console::registros::{leer_cola, redactar, CONTEXTO, MARCA};
use ops_console::sistema::{presencia, Entorno, EntornoDelProceso, EntornoFijo, VARIABLES};

const R: &str = MARCA;

fn r(texto: &str) -> String {
    redactar(texto).como_str().to_string()
}

/// Cada caso: (entrada, salida esperada).
fn casos() -> Vec<(String, String)> {
    let c = |a: &str, b: &str| (a.to_string(), b.replace("[R]", R));
    vec![
        // Correos.
        c("aviso para ana.lopez@example.com hoy", "aviso para [R] hoy"),
        c("ALGUIEN+FILTRO@EXAMPLE.ORG", "[R]"),
        c("correo: <soporte@example.net>.", "correo: <[R]>."),
        c("josé.núñez@ejemplo.test escribió", "[R] escribió"),
        // Tokens con clave.
        c("token=abc123", "token=[R]"),
        c("GET /x?token=abc123&lado=256", "GET /x?token=[R]&lado=256"),
        c("access_token: eyJhbGci.carga.firma", "access_token: [R]"),
        c(
            "{\"token\": \"abc 123\", \"lado\": 256}",
            "{\"token\": \"[R]\", \"lado\": 256}",
        ),
        c("{'secret': 'abc def'}", "{'secret': '[R]'}"),
        c(
            "{\"token\": \"con \\\"comillas\\\" dentro\"}",
            "{\"token\": \"[R]\"}",
        ),
        c("password=hunter2 y sigue", "password=[R] y sigue"),
        c("PASSWORD = hunter2", "PASSWORD = [R]"),
        c("db_passwd=x9", "db_passwd=[R]"),
        c("contraseña: mi clave de siempre", "contraseña: [R]"),
        c("CONTRASEÑA: x", "CONTRASEÑA: [R]"),
        c("MINIATURAS_CLAVE_API=sk-abc", "MINIATURAS_CLAVE_API=[R]"),
        c("--api-key=k1 --lado 256", "--api-key=[R] --lado 256"),
        c("Cookie: sesion=abc; tema=oscuro", "Cookie: [R]"),
        // Claves agregadas después del lote 5: pwd, passphrase, credencial, private_key, cookie.
        c("password=x y pwd: x", "password=[R] y pwd: [R]"),
        c("pwd=x9k2", "pwd=[R]"),
        c("PWD: dos palabras", "PWD: [R]"),
        c("{\"pwd\": \"abc def\"}", "{\"pwd\": \"[R]\"}"),
        c("pwd='abc'", "pwd='[R]'"),
        c("pwd:\n  abc", "pwd:\n  [R]"),
        c("passphrase=caballo-correcto", "passphrase=[R]"),
        c(
            "PASSPHRASE: caballo correcto batería grapa",
            "PASSPHRASE: [R]",
        ),
        c(
            "{\"passphrase\":\n  \"caballo correcto\"}",
            "{\"passphrase\":\n  \"[R]\"}",
        ),
        c("credencial=abc123", "credencial=[R]"),
        c("Credentials: usuario:clave", "Credentials: [R]"),
        c("{\"credential\": \"abc\"}", "{\"credential\": \"[R]\"}"),
        c("CREDENTIAL=\nabc123", "CREDENTIAL=\n[R]"),
        c("private_key: abc", "private_key: [R]"),
        c("PRIVATE-KEY=abc", "PRIVATE-KEY=[R]"),
        c("{\"privateKey\": \"abc\"}", "{\"privateKey\": \"[R]\"}"),
        c("clave_privada=abc", "clave_privada=[R]"),
        c("set-cookie: sesion=abc", "set-cookie: [R]"),
        c("COOKIE=abc", "COOKIE=[R]"),
        // Sobrerredacción asumida: la variable del directorio actual también se tapa.
        c("PWD=/srv/miniaturas", "PWD=[R]"),
        c(
            "Authorization: Basic dXN1YXJpbzpjbGF2ZQ==",
            "Authorization: [R]",
        ),
        c("Authorization: Bearer abc.def.ghi", "Authorization: [R]"),
        // Bearer suelto y claves con prefijo.
        c(
            "con bearer abc123 en minúsculas",
            "con bearer [R] en minúsculas",
        ),
        c("BEARER xyz, siguiente", "BEARER [R], siguiente"),
        c(
            "clave sk-proj-abcdefgh1234 filtrada",
            "clave sk-[R] filtrada",
        ),
        c("SK_LIVE_ABCDEFGH12345678", "SK_[R]"),
        c("ghp_0123456789abcdefABCDEF.", "ghp_[R]."),
        // Identificadores de cuenta.
        c(
            "cuota agotada para CLI-482913",
            "cuota agotada para CLI-[R]",
        ),
        c("cli-0001 y CLI-1234567890", "cli-[R] y CLI-[R]"),
        // Rutas sensibles.
        c(
            "leyendo /etc/secretos/firma.pem",
            "leyendo /etc/secretos/[R]",
        ),
        c("/ETC/SECRETOS/X y más", "/ETC/SECRETOS/[R] y más"),
        c("C:\\Secretos\\firma.pem", "C:\\Secretos\\[R]"),
        c("c:/secretos/clave.txt", "c:/secretos/[R]"),
        c(
            "\"D:\\\\Secretos\\\\firma.pem\"",
            "\"D:\\\\Secretos\\\\[R]\"",
        ),
        // Credenciales en URL.
        c(
            "https://lector:clave@origen.example/x.jpg",
            "https://[R]@origen.example/x.jpg",
        ),
        c("FTP://USUARIO:CLAVE@HOST.EXAMPLE", "FTP://[R]@HOST.EXAMPLE"),
        c(
            "https://origen.example/v1/cuentas/77120034/miniaturas",
            "https://origen.example/v1/cuentas/[R]/miniaturas",
        ),
        c("GET /CUENTAS/ABC-123?x=1", "GET /CUENTAS/[R]?x=1"),
    ]
}

#[test]
fn se_redactan_muchas_formas() {
    for (entrada, esperada) in casos() {
        assert_eq!(r(&entrada), esperada, "entrada: {entrada:?}");
    }
}

#[test]
fn un_valor_en_la_linea_siguiente_tambien_se_redacta() {
    for (entrada, esperada) in [
        ("token=\nvalor-en-la-siguiente", "token=\n[R]"),
        ("token:\r\n    valor-con-crlf\r\n", "token:\r\n    [R]\r\n"),
        (
            "\"password\":\n    \"abc\"\n",
            "\"password\":\n    \"[R]\"\n",
        ),
        ("Bearer\nabc123", "Bearer\n[R]"),
        ("BEARER\r\n  abc123 fin", "BEARER\r\n  [R] fin"),
        ("token=abc\r\nsiguiente", "token=[R]\r\nsiguiente"),
    ] {
        assert_eq!(
            r(entrada),
            esperada.replace("[R]", R),
            "entrada: {entrada:?}"
        );
    }
}

#[test]
fn ni_mayusculas_ni_minusculas_dejan_pasar_un_secreto() {
    for (entrada, _) in casos() {
        for variante in [entrada.to_uppercase(), entrada.to_lowercase()] {
            let salida = r(&variante);
            assert!(salida.contains(R), "{variante:?} → {salida:?}");
        }
    }
}

#[test]
fn el_texto_que_no_es_sensible_queda_igual() {
    for linea in [
        "",
        "2026-10-06T23:40:01Z INFO  miniaturas::cola  tarea 18234 lista: 4000x3000 -> 400x300 en 42 ms",
        "INFO  miniaturas::cache  guardada portada@2x.png (31 KB)",
        "guardada miniatura-18301@3X.webp",
        "WARN  la tarea 18240 tardó 1250 ms en leer el original",
        "GET /v1/miniaturas/8812?lado=256 -> 200",
        "https://origen.example/imagenes/8812.jpg",
        "ruta /etc/miniaturas/config.toml leída",
        "ruta /srv/secretos/x no es de configuración",
        "C:\\Programas\\miniaturas\\miniaturas.exe",
        "el token caducó; se pide otro",
        "si token == nulo, se pide otro",
        "versión 2.0 en desk-top, sk-corto",
        "la cuenta CLI-12 es de prueba",
        "tiempo: 10:30:00",
        "líneas con acentos: ñandú, pingüino, café",
        "ya estaba tapado: [REDACTADO]",
    ] {
        assert_eq!(r(linea), linea);
    }
}

#[test]
fn redactar_dos_veces_da_lo_mismo_que_una() {
    for (entrada, _) in casos() {
        let una = r(&entrada);
        assert_eq!(r(&una), una, "entrada: {entrada:?}");
    }
}

#[test]
fn la_redaccion_no_agrega_ni_quita_lineas() {
    for (entrada, _) in casos() {
        assert_eq!(
            r(&entrada).matches('\n').count(),
            entrada.matches('\n').count()
        );
    }
}

/// Un generador determinista (xorshift), para no depender del azar del sistema.
struct Semilla(u64);

impl Semilla {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn hasta(&mut self, n: usize) -> usize {
        usize::try_from(self.siguiente() % u64::try_from(n).expect("n")).expect("índice")
    }

    fn valor(&mut self) -> String {
        (0..10)
            .map(|_| char::from(b"abcdefghjkmnpqrstuvwxyz23456789"[self.hasta(31)]))
            .collect()
    }

    fn digitos(&mut self) -> String {
        (0..7)
            .map(|_| char::from(b'0' + u8::try_from(self.hasta(10)).expect("dígito")))
            .collect()
    }
}

#[test]
fn un_corpus_generado_no_deja_escapar_ningun_secreto() {
    let mut s = Semilla(0x5eed_1234_abcd_0042);
    let separadores = [" ", "\n", "\r\n", ", ", " | ", "\t"];
    let relleno = [
        "INFO",
        "miniaturas::api",
        "tarea 12 lista",
        "ok",
        "->",
        "200",
    ];
    for _ in 0..2000 {
        let mut texto = String::new();
        let mut secretos = Vec::new();
        for _ in 0..1 + s.hasta(5) {
            let v = s.valor();
            let mayus = s.hasta(2) == 0;
            let k = |clave: &str| {
                if mayus {
                    clave.to_uppercase()
                } else {
                    clave.to_string()
                }
            };
            let fragmento = match s.hasta(15) {
                0 => format!("{}={v}", k("token")),
                1 => format!("{} = {v}", k("password")),
                2 => format!("{} {v}", k("Bearer")),
                3 => format!("{}\n{v}", k("bearer")),
                4 => format!("\"{}\": \"{v}\"", k("secret")),
                5 => format!("{}:{v}", k("api_key")),
                6 => format!("{}{v}@origen.example/x", k("https://u:")),
                7 => format!("{}{v}/x", k("/cuentas/")),
                8 => format!("{}{v}", k("/etc/secretos/")),
                9 => format!("{}{v}", k("C:\\Secretos\\")),
                10 => format!("{}{v}", k("sk-")),
                11 => format!("{v}@{}", k("example.com")),
                12 => format!("{}: {v}", k("pwd")),
                13 => format!("\"{}\": \"{v}\"", k("passphrase")),
                _ => {
                    let d = s.digitos();
                    let fragmento = format!("{}{d}", k("CLI-"));
                    secretos.push(d);
                    texto.push_str(&fragmento);
                    texto.push_str(separadores[s.hasta(separadores.len())]);
                    continue;
                }
            };
            secretos.push(v);
            texto.push_str(&fragmento);
            texto.push_str(separadores[s.hasta(separadores.len())]);
            texto.push_str(relleno[s.hasta(relleno.len())]);
            texto.push_str(separadores[s.hasta(separadores.len())]);
        }
        let salida = r(&texto);
        for secreto in &secretos {
            assert!(
                !salida.contains(secreto.as_str()),
                "se escapó {secreto:?} de {texto:?} → {salida:?}"
            );
        }
        assert_eq!(r(&salida), salida, "idempotencia: {texto:?}");
        assert_eq!(salida.matches('\n').count(), texto.matches('\n').count());
    }
}

#[test]
fn un_redactado_se_parte_en_lineas_redactadas() {
    let texto = redactar("uno token=a\r\ndos\n\ntres\n");
    let lineas: Vec<String> = texto.lineas().iter().map(ToString::to_string).collect();
    assert_eq!(
        lineas,
        [
            format!("uno token={R}"),
            "dos".into(),
            String::new(),
            "tres".into()
        ]
    );
    assert!(redactar("").lineas().is_empty());
    assert_eq!(texto.to_string(), texto.como_str());
}

// ─── La cola del archivo ──────────────────────────────────────────────────────────────────

#[test]
fn la_cola_de_un_archivo_chico_son_sus_ultimas_lineas() {
    let carpeta = Carpeta::nueva("cola-chica");
    let contenido: String = (1..=10).map(|i| format!("línea {i}\n")).collect();
    let ruta = carpeta.archivo("chico.log", contenido.as_bytes());
    let cola = leer_cola(&ruta, 3, 1024 * 1024).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, ["línea 8", "línea 9", "línea 10"]);
    assert!(!cola.recortado);
    assert_eq!(cola.bytes_leidos, cola.bytes_archivo);
}

#[test]
fn un_archivo_enorme_se_lee_desde_el_final_y_nunca_entero() {
    let carpeta = Carpeta::nueva("cola-enorme");
    let mut contenido = String::from("token=SECRETO-DEL-PRINCIPIO\n");
    let mut i = 0;
    while contenido.len() < 4 * 1024 * 1024 {
        contenido.push_str(&format!(
            "INFO  miniaturas::cola  tarea {i} lista: 4000x3000 -> 400x300\n"
        ));
        i += 1;
    }
    contenido.push_str("ERROR al final, para ana@example.com\n");
    let ruta = carpeta.archivo("enorme.log", contenido.as_bytes());
    let tope = 64 * 1024;

    let cola = leer_cola(&ruta, 50, tope).expect("leer");
    assert!(cola.recortado);
    assert_eq!(
        cola.bytes_archivo,
        u64::try_from(contenido.len()).expect("largo")
    );
    assert_eq!(
        cola.bytes_leidos,
        tope + CONTEXTO,
        "leyó la ventana y su contexto, nada más"
    );
    assert_eq!(cola.lineas.len(), 50);
    assert_eq!(
        cola.lineas[49].to_string(),
        format!("ERROR al final, para {R}")
    );
    assert!(cola
        .lineas
        .iter()
        .all(|l| !l.como_str().contains("SECRETO")));
}

#[test]
fn la_linea_que_corta_la_ventana_no_se_muestra() {
    let carpeta = Carpeta::nueva("cola-corte");
    let ruta = carpeta.archivo("corte.log", b"primera-linea-larga\nsegunda\ntercera\n");
    // La ventana de 20 bytes empieza a mitad de «primera-linea-larga».
    let cola = leer_cola(&ruta, 100, 20).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, ["segunda", "tercera"]);
    // Una ventana que empieza justo al comienzo de una línea la muestra entera.
    let cola = leer_cola(&ruta, 100, 16).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, ["segunda", "tercera"]);
    let cola = leer_cola(&ruta, 100, 8).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, ["tercera"]);
}

#[test]
fn finales_crlf_sin_salto_final_vacio_e_inexistente() {
    let carpeta = Carpeta::nueva("cola-bordes");
    let ruta = carpeta.archivo("crlf.log", b"uno\r\ndos\r\ntres");
    let cola = leer_cola(&ruta, 10, 1024).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, ["uno", "dos", "tres"]);

    let vacio = carpeta.archivo("vacio.log", b"");
    assert!(leer_cola(&vacio, 10, 1024).expect("leer").lineas.is_empty());

    let error = leer_cola(&carpeta.ruta().join("no-existe.log"), 10, 1024).expect_err("no existe");
    assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn una_clave_que_quedo_antes_del_corte_todavia_alcanza_a_su_valor() {
    let carpeta = Carpeta::nueva("cola-contexto");
    let antes = "INFO  renovando token=\n";
    let valor = "valor-que-no-debe-verse\n";
    let despues = "INFO  listo\n";
    let ruta = carpeta.archivo(
        "contexto.log",
        format!("{antes}{valor}{despues}").as_bytes(),
    );
    // La ventana empieza justo en la línea del valor: su clave quedó afuera.
    let tope = u64::try_from(valor.len() + despues.len()).expect("tope");
    let cola = leer_cola(&ruta, 10, tope).expect("leer");
    let lineas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
    assert_eq!(lineas, [R.to_string(), "INFO  listo".to_string()]);
}

#[test]
fn cualquier_punto_de_corte_redacta_igual_que_leer_el_archivo_entero() {
    let carpeta = Carpeta::nueva("cola-cortes");
    let mut contenido = String::new();
    let mut i = 0;
    while contenido.len() < 3 * 1024 {
        contenido.push_str(&match i % 7 {
            0 => format!("INFO  tarea {i} lista: 4000x3000 -> 400x300\n"),
            1 => format!("DEBUG renovando token=\n      valor-{i}-en-la-siguiente\n"),
            2 => format!("WARN  reintento contra https://u:clave{i}@origen.example/x.jpg\n"),
            3 => format!("INFO  petición de persona{i}@example.com, Bearer\n  portador{i}\n"),
            4 => format!("DEBUG {{\"password\":\n    \"secreto{i}\"}}\n"),
            5 => format!("ERROR cuota agotada para CLI-77{i:04}, ver /etc/secretos/clave{i}.pem\n"),
            _ => format!("INFO  GET /v1/cuentas/99{i}/uso con sk-proyecto{i:08}\n"),
        });
        i += 1;
    }
    let ruta = carpeta.archivo("cortes.log", contenido.as_bytes());
    let completo: Vec<String> = redactar(&contenido)
        .lineas()
        .iter()
        .map(ToString::to_string)
        .collect();
    let inicios: Vec<usize> = std::iter::once(0)
        .chain(contenido.match_indices('\n').map(|(p, _)| p + 1))
        .filter(|&p| p < contenido.len())
        .collect();
    assert_eq!(inicios.len(), completo.len());

    for tope in 1..=contenido.len() {
        let ventana = contenido.len() - tope;
        let esperadas: Vec<&String> = completo
            .iter()
            .zip(&inicios)
            .filter(|(_, &inicio)| inicio >= ventana)
            .map(|(linea, _)| linea)
            .collect();
        let cola = leer_cola(&ruta, usize::MAX, u64::try_from(tope).expect("tope")).expect("leer");
        let obtenidas: Vec<String> = cola.lineas.iter().map(ToString::to_string).collect();
        assert_eq!(
            obtenidas.iter().collect::<Vec<_>>(),
            esperadas,
            "con una ventana de {tope} bytes"
        );
        for linea in &obtenidas {
            // El directorio sensible se muestra; lo que cuelga de él, no.
            let sin_directorio = linea.replace("/etc/secretos/", "/etc/");
            for rastro in [
                "valor-", "clave", "persona", "portador", "secreto", "CLI-77", "proyecto",
            ] {
                assert!(!sin_directorio.contains(rastro), "{rastro} en {linea:?}");
            }
        }
    }
}

// ─── Variables definidas, sin valores ─────────────────────────────────────────────────────

#[test]
fn la_presencia_de_cada_variable_es_un_si_o_un_no() {
    let entorno = EntornoFijo::default()
        .con("MINIATURAS_CLAVE_API", "sk-valor-secreto")
        .con("MINIATURAS_DIR_CACHE", "")
        .con("OTRA_VARIABLE", "x");
    let presencia = presencia(&entorno);
    assert_eq!(
        presencia.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        VARIABLES
    );
    for (nombre, definida) in presencia {
        assert_eq!(
            definida,
            nombre == "MINIATURAS_CLAVE_API",
            "{nombre}: una vacía no cuenta"
        );
    }
    assert!(EntornoDelProceso.definida("PATH"));
    assert!(!EntornoDelProceso.definida("OPS_CONSOLE_VARIABLE_QUE_NO_EXISTE_9F3A"));
}
