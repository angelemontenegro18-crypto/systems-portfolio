# polite-crawler

**Un crawler honesto, en Rust: dice quién es, pide permiso y no apura a nadie.**

Identidad declarada con un contacto, `robots.txt` interpretado según la RFC 9309 — sin
ninguna forma de ignorarlo —, límites por origen, `Crawl-delay`, `Retry-After` y backoff.
Pensado para agentes y herramientas que leen la web de otros y quieren hacerlo bien.

```rust
let identidad = Identidad::nueva("LectorDocs", "0.1", "https://ejemplo.test/bot")?;
let mut rastreador = Rastreador::nuevo(TransporteHttp::nuevo(timeout, max_bytes), identidad, Config::default());

match rastreador.obtener("https://sitio.test/guia", ahora_ms) {
    Ok(pagina) => { /* … */ }
    Err(Rechazo::Esperar { desde_ms, .. }) => { /* volver a intentar desde ahí */ }
    Err(Rechazo::ProhibidoPorRobots { motivo }) => { /* p. ej. "Disallow: /privado" */ }
    Err(otro) => { /* … */ }
}
```

---

## Lo que este crawler no hace, por diseño

Su valor está en ser lo contrario de un scraper que intenta pasar desapercibido:

- **No evade la detección de bots.** Se presenta siempre como lo que es.
- **No falsea el User-Agent.** La única forma de construirlo es `Identidad::nueva`, que
  exige nombre, versión y un **contacto** (URL o correo) para que quien administra el sitio
  sepa quién lo lee y cómo pedirle que pare. El nombre no puede imitar a un navegador ni al
  bot conocido de otra organización.
- **No rota identidades.** La identidad se fija al crear el rastreador y no cambia.
- **No sale por proxies** ni oculta de dónde vienen las peticiones.
- **No se puede configurar para ignorar `robots.txt`.** No existe la opción: el tipo no la
  tiene.
- **No aleatoriza sus tiempos.** Las esperas son deterministas: solo alargan, nunca
  acortan, lo que el sitio pide.
- **Solo hace `GET`.** No envía formularios ni inicia sesiones.

Un test (`tests/sin_evasion.rs`) recorre el código fuente y falla si aparece cualquier
mecanismo de ese tipo, verifica que las dependencias sean exactamente las declaradas, y
demuestra que el escaneo detectaría una violación.

## Qué demuestra

- **RFC 9309 completa.** Grupos por token de producto sin distinguir mayúsculas, grupos
  del mismo agente combinados, comodín `*` y ancla `$`, la regla más específica gana y
  ante un empate gana `Allow`, `/robots.txt` siempre permitido, rutas comparadas con la
  misma codificación en porcentaje, y un límite de 500 KiB al parsear.
- **Lo que la RFC exige ante fallas.** Un `robots.txt` con 4xx significa "sin reglas".
  Con 5xx, 429 o sin red, **se asume todo prohibido** hasta reintentar. Las redirecciones
  de `robots.txt` se siguen hasta cinco saltos, y lo leído vale como mucho 24 horas.
- **Leer `robots.txt` cuenta como una petición.** Ni siquiera eso se salta el intervalo del
  host: la primera página de un origen llega en la llamada siguiente.
- **Las redirecciones no se siguen solas.** Una redirección puede llevar a otro host, con
  su propio `robots.txt` y su propio límite. Se devuelve, y el destino se pide aparte,
  pasando por todas las reglas.
- **Nunca duerme.** Cuando todavía no toca, devuelve `Esperar { desde_ms }`. El reloj se
  inyecta, así que toda la cortesía se prueba de forma determinista, sin red ni esperas.
- **Cero `unsafe`**, dos dependencias (`ureq` para HTTP/TLS y `url`).

## El orden de las decisiones

```
obtener(url, ahora)
  │
  ├─ ¿robots.txt de este origen leído y vigente?  no → leerlo (cuenta como petición)
  ├─ ¿robots.txt permite la ruta?                 no → ProhibidoPorRobots { regla }
  ├─ ¿queda presupuesto de páginas?               no → PresupuestoAgotado
  ├─ ¿pasó el intervalo / Crawl-delay / backoff?  no → Esperar { desde_ms }
  └─ GET  →  429, 5xx o falla de red: Retry-After (en segundos) o backoff exponencial
```

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Cada regla de la RFC 9309 | `tests/robots.rs` (15 tests) |
| Una ruta prohibida nunca se pide, y el rechazo cita la regla | `una_ruta_prohibida_se_rechaza_con_su_regla_y_nunca_se_pide` |
| `robots.txt` con 5xx, 429 o sin red: todo prohibido, y se reintenta después | `robots_con_5xx_429_o_inalcanzable_prohibe_todo_y_se_reintenta_despues` |
| Leer `robots.txt` cuenta para el intervalo | `leer_robots_cuenta_como_peticion_y_la_pagina_llega_en_la_siguiente` |
| `Crawl-delay` alarga el intervalo y nunca lo acorta | `crawl_delay_alarga_el_intervalo_pero_nunca_lo_acorta` |
| `Retry-After` se respeta tal cual | `retry_after_se_respeta_tal_cual_aunque_supere_el_tope_del_backoff` |
| El backoff se duplica, tiene tope y se reinicia con un éxito | `el_backoff_se_duplica_tiene_tope_y_se_reinicia_con_un_exito` |
| Una redirección no se sigue sola | `una_redireccion_se_devuelve_y_no_se_sigue_sola`, `el_transporte_no_sigue_redirecciones` |
| Cada petición lleva la misma identidad declarada | `cada_peticion_lleva_la_misma_identidad_declarada` |
| El User-Agent que llega al servidor es el declarado | `el_user_agent_que_llega_al_servidor_es_la_identidad_declarada` (HTTP real) |
| La identidad exige contacto y no imita a otros | `identidad::tests` |
| El código no contiene mecanismos de evasión | `tests/sin_evasion.rs` |

Los tests de cortesía usan un transporte falso y un reloj controlado. Los de HTTP corren
contra un servidor real en el mismo proceso.

## Correr

```bash
cargo test                  # 45 tests
cargo run --example demo    # recorre un sitio simulado en el mismo proceso
```

```
Me presento como: LectorDemo/0.1 (+https://ejemplo.test/bot)

   600 ms  (esperé por CrawlDelay)
   601 ms  /                                200 · "página /"
   601 ms  /borradores/idea                 no: robots.txt no lo permite (Disallow: /borradores/)
  1201 ms  /borradores/publicados/manual    200 · "página /borradores/publicados/manual"
  1800 ms  /viejo                           301 → redirige a /noticias (se pide aparte)
  2401 ms  /noticias                        429 → el servidor pide esperar hasta 4401 ms
  4402 ms  /noticias                        200 · "página /noticias"
```

## Límites conocidos

- **`Retry-After` en forma de fecha HTTP no se interpreta**; en ese caso se aplica el
  backoff exponencial. La forma en segundos, la habitual, se respeta tal cual.
- **Las redirecciones de `robots.txt`** se siguen dentro de la misma llamada, sin esperar el
  intervalo entre saltos (son pocas peticiones pequeñas y la RFC exige seguirlas).
- **Un rastreador por proceso.** El estado por origen vive en memoria; varios procesos
  rastreando el mismo sitio tienen que coordinarse afuera.
- **No descarga recursos de la página** (imágenes, scripts, estilos) ni ejecuta JavaScript:
  lee documentos.

## Licencia

MIT — ver [LICENSE](LICENSE).
