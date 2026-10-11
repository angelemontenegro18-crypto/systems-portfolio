# ops-console

**Una consola de operaciones local, con su propio servidor HTTP escrito sobre `std`: solo
puede escuchar en loopback, no se deja usar por otras páginas del navegador, supervisa sus
procesos sin dejar huérfanos y muestra los registros con lo sensible tapado.**

Un servicio de miniaturas (inventado) redimensiona imágenes con un grupo de procesos
trabajadores. La consola corre en la misma máquina y sirve una página para operarlo: ver,
reciclar y escalar los trabajadores, el estado de sus servicios de systemd, la cola de su
registro y qué configuración tiene definida. Una consola «local» suele fallar por cuatro
lados: escucha en todas las interfaces por un descuido; cualquier página abierta en el
navegador del operador puede hablarle (rebinding de DNS, CSRF); el registro que muestra filtra
secretos; y un proceso que muere de golpe deja hijos huérfanos. Este crate cierra cada uno con
tipos y con pruebas.

```rust
let direccion = DireccionLocal::desde_texto("127.0.0.1:8750")?;   // «0.0.0.0:8750» → Err(NoLocal)
let servidor = Servidor::escuchar(direccion, Limites::default(), consola)?;
// Servidor::escuchar("0.0.0.0:8750".parse()?, …)  → no compila: se espera una DireccionLocal

let cola = leer_cola(&registro, 100, 256 * 1024)?;   // lee el final del archivo, nunca entero
let linea: &Redactado = &cola.lineas[0];             // solo `redactar()` crea un Redactado
```

---

## Qué demuestra

- **Solo loopback, por tipo.** `Servidor::escuchar` recibe una `DireccionLocal`, que solo
  existe para `127.0.0.0/8` o `::1`: pasarle un `SocketAddr` no compila (`compile_fail,E0308`)
  y construirla a mano tampoco (`compile_fail,E0451`). `localhost` se toma como `127.0.0.1` sin
  preguntarle a ningún resolvedor. `0.0.0.0`, una IP pública o `localhots` se rechazan antes de
  abrir nada, y el binario se niega a arrancar y dice por qué.
- **HTTP estricto, con límites.** Una petición por conexión (`Connection: close`, sin
  keep-alive ni *chunked*). Largo de la línea de petición (414), bytes y cantidad de cabeceras
  (431), cuerpo (413, decidido con solo mirar `Content-Length`, antes de leer un byte), plazo
  total para recibir la petición (408: un cliente que gotea un byte cada 50 ms no lo estira) y
  conexiones simultáneas (503). Los límites se vigilan mientras llegan los bytes, no al final.
- **Sin contrabando de peticiones.** `Transfer-Encoding`, en cualquier forma, es 501.
  `Content-Length` repetido (aunque sea igual), con signo, comas o espacios es 400, y un `POST`
  sin él es 411. Cabeceras plegadas, un espacio antes de los dos puntos o un fin de línea sin
  `\r` también son 400.
- **El navegador no es cómplice.** `Host` tiene que ser uno de los nombres del propio servidor
  (421): un rebinding de DNS llega a `127.0.0.1` pero con `Host: atacante.example`. Todo `POST`
  exige `Content-Type: application/json` (415) y la cabecera `X-Consola: 1` (403); si llegan
  `Origin` o `Sec-Fetch-Site`, tienen que ser del mismo origen (403). Todo eso se comprueba
  antes de leer el cuerpo. Ninguna respuesta lleva cabeceras CORS, y un *preflight* recibe 405.
- **Cabeceras de seguridad en toda respuesta**, también en los errores: CSP
  `default-src 'self'` (sin `unsafe-inline`), `nosniff`, `Referrer-Policy: no-referrer`,
  `X-Frame-Options: DENY`, `Cache-Control: no-store`, COOP, CORP y `Permissions-Policy`.
- **Un grupo sin huérfanos.** Cada posición tiene un id estable. Reciclar lanza el reemplazo
  antes de terminar al viejo, y la posición conserva su lugar y su id. Escalar respeta el mínimo
  y el máximo de la política. El tick reemplaza lo que murió y recicla lo que cumplió su tiempo
  de vida, y dice qué hizo. Soltar el grupo —también al desenrollar un pánico— termina y espera
  a todos. Los trabajadores terminan cuando se cierra su entrada, así que ni con la consola
  muerta de golpe quedan vivos.
- **Servicios con tiempo máximo y caché.** `systemctl show` tiene 3 s de límite: una consulta
  colgada se corta y se espera. Cada respuesta se recuerda 10 s por unidad. El reloj se inyecta,
  así que las pruebas fijan los dos plazos sin dormir. Fuera de Linux, los servicios se informan
  como «no disponible», con el motivo.
- **Registros sin secretos.** La cola se lee desde el final, con un tope (256 KiB más 1 KiB de
  contexto); un archivo enorme no se lee entero. Se tapan:
  - correos;
  - tokens (`token=…`, `Bearer …`, claves `sk-…`);
  - el valor de toda clave cuyo nombre lleve `password`, `passwd`, `pwd`, `passphrase`,
    `contraseña`, `clave`, `secret`, `credencial`/`credential`, `private_key`, `api_key`,
    `authorization` o `cookie` (`DB_PASSWORD=…`, `pwd: …`, `"privateKey": "…"`);
  - identificadores de cuenta (`CLI-123456`);
  - lo que cuelga de `/etc/secretos/` o `C:\Secretos\`;
  - credenciales en una URL (`https://usuario:clave@…`, `/cuentas/<id>/`).

  Da igual si están en mayúsculas o si el valor quedó en la línea siguiente a su clave.
  Redactar dos veces da lo mismo que una. Lo que sale por la API es un `Redactado`, un tipo que
  solo crea `redactar()`: devolver texto crudo no compila (`compile_fail,E0451` y `E0308`).
- **Configuración sin valores.** `/api/sistema` dice, de cada variable, si está definida o no.
  El rasgo `Entorno` solo sabe responder eso, así que no hay de dónde sacar un valor.
- **Una interfaz sin por dónde inyectar.**
  - HTML, CSS y JS propios, sin bibliotecas ni recursos externos, incrustados en el binario:
    nada se sirve desde el disco.
  - Todo dato entra al documento como texto (`textContent`, `createElement`).
  - Un escáner revisa los archivos en busca de `innerHTML`, `eval`, scripts, estilos o
    manejadores en línea y recursos de otros orígenes, y una meta-prueba comprueba que el
    escáner detecta cada uno.
  - Tema claro y oscuro (`prefers-color-scheme`), se adapta a pantallas angostas, se recorre
    con el teclado y el foco se ve.
- **Sin dependencias.** El HTTP y el JSON también están escritos sobre `std`. El JSON que se lee
  es estricto (sin claves repetidas, sin decimales, hasta 8 niveles) y el que se escribe escapa
  `<`, `>`, `&` y U+2028/2029. `#![forbid(unsafe_code)]`.

## Garantías y cómo se prueban

| Garantía | Prueba |
|---|---|
| Escuchar exige una `DireccionLocal`, que no se fabrica a mano | doctests `compile_fail,E0308` (`servidor`) y `compile_fail,E0451` (`direccion`) |
| `0.0.0.0`, una IP pública y un `localhost` mal escrito se rechazan antes de abrir nada | `cero_una_ip_publica_y_un_localhost_mal_escrito_se_rechazan_antes_de_abrir_nada`, `el_binario_se_niega_a_escuchar_fuera_de_loopback_y_dice_por_que` |
| Límites de línea, cabeceras y cuerpo (414, 431, 413) | `una_linea_de_peticion_demasiado_larga_es_414`, `una_linea_larga_se_rechaza_sin_esperar_al_final_de_la_cabeza`, `cabeceras_que_superan_el_tope_de_bytes_son_431`, `demasiadas_cabeceras_son_431`, `un_cuerpo_anunciado_mayor_que_el_tope_es_413_sin_leer_un_byte` |
| Un `Content-Length` mentiroso o ausente no hace leer de más | `con_un_content_length_mentiroso_no_se_lee_de_mas`, `un_cuerpo_que_promete_mas_de_lo_que_llega_vence_el_plazo_408`, `un_post_sin_content_length_es_411` |
| Plazo total de lectura (408), también con goteo, y el lugar se libera | `un_cliente_mudo_recibe_408_al_vencer_el_plazo`, `un_cliente_que_gotea_bytes_no_estira_el_plazo_ni_retiene_su_lugar` |
| Tope de conexiones (503) | `pasado_el_tope_de_conexiones_la_siguiente_recibe_503` |
| Una petición por conexión | `una_peticion_correcta_recibe_su_respuesta_y_la_conexion_se_cierra`, `una_segunda_peticion_en_la_misma_conexion_no_se_atiende` |
| Sin contrabando de peticiones | `content_length_repetido_o_en_conflicto_es_400`, `transfer_encoding_en_cualquier_forma_es_501`, `lo_que_no_se_entiende_entero_se_rechaza` |
| Un `Host` ajeno es 421 | `un_host_ajeno_es_421_aunque_la_conexion_llegue_a_loopback`, `los_nombres_propios_del_servidor_se_aceptan`, `en_el_puerto_80_el_host_tambien_vale_sin_puerto` |
| Todo `POST`: JSON, cabecera propia y mismo origen, antes del cuerpo | `un_post_que_no_es_json_es_415`, `un_post_sin_la_cabecera_propia_es_403`, `un_post_de_otro_origen_es_403`, `un_post_del_mismo_origen_pasa`, `las_defensas_se_aplican_antes_de_leer_el_cuerpo` |
| Sin CORS, solo `GET` y `POST` | `solo_get_y_post_y_sin_preflight_cors` |
| Cabeceras de seguridad en cada ruta y en cada error | `cada_ruta_lleva_las_cabeceras_de_seguridad_y_ninguna_de_cors` (26 respuestas), `los_errores_tambien_cierran_y_llevan_las_cabeceras_de_seguridad` |
| Reciclar conserva lugar e id, y lanza antes de terminar | `reciclar_conserva_la_posicion_y_el_id`, `reciclar_lanza_el_reemplazo_antes_de_terminar_al_viejo`, `si_el_reemplazo_no_arranca_el_viejo_sigue_en_su_lugar`, `reciclar_por_la_api_conserva_la_posicion` |
| Escalar respeta la política; una política inválida es 400 y no cambia nada | `escalar_respeta_el_minimo_y_el_maximo`, `escalar_por_la_api_respeta_el_minimo_y_el_maximo`, `una_politica_invalida_es_400_y_no_cambia_nada` |
| El tick reemplaza lo muerto y lo vencido, y lo informa | `el_tick_reemplaza_lo_que_murio_en_su_lugar_y_dice_que_hizo`, `el_tick_recicla_lo_que_cumplio_su_tiempo_de_vida`, `un_reemplazo_fallido_queda_anotado_y_el_siguiente_tick_lo_reintenta`, `el_tick_reemplaza_lo_que_murio_y_queda_anotado` |
| Ningún huérfano, ni en un pánico | `soltar_el_grupo_termina_todos_sus_procesos`, `ni_un_panico_deja_procesos_sin_terminar`, `de_punta_a_punta_con_procesos_reales` |
| El vigía corta a los 3 s y recuerda 10 s, sin dormir | `una_consulta_colgada_se_corta_a_los_tres_segundos_sin_dormir_de_verdad`, `la_cache_guarda_cada_respuesta_diez_segundos_por_unidad` |
| Redacción en todas sus formas, mayúsculas y saltos de línea, idempotente | `se_redactan_muchas_formas` (56 formas), `ni_mayusculas_ni_minusculas_dejan_pasar_un_secreto`, `un_valor_en_la_linea_siguiente_tambien_se_redacta`, `redactar_dos_veces_da_lo_mismo_que_una`, `un_corpus_generado_no_deja_escapar_ningun_secreto` |
| Lo que no es sensible queda igual | `el_texto_que_no_es_sensible_queda_igual` (17 líneas) |
| La cola se lee desde el final, nunca entera, y el corte no deja escapar nada | `un_archivo_enorme_se_lee_desde_el_final_y_nunca_entero`, `cualquier_punto_de_corte_redacta_igual_que_leer_el_archivo_entero`, `una_clave_que_quedo_antes_del_corte_todavia_alcanza_a_su_valor` |
| La API solo devuelve texto `Redactado` | doctests `compile_fail,E0451` y `compile_fail,E0308` (`registros`), `los_registros_salen_redactados` |
| `/api/sistema`: sí o no, nunca el valor | `el_sistema_dice_si_cada_variable_esta_definida_y_nunca_su_valor` (cada variable sola, todas y ninguna) |
| Todos los errores tienen la misma forma | `todos_los_errores_tienen_la_misma_forma`, `reciclar_rechaza_entero_lo_que_no_entiende` |
| La interfaz no tiene por dónde inyectar | `ningun_archivo_de_la_interfaz_tiene_nada_prohibido`, `el_escaner_encuentra_cada_cosa_prohibida`, `el_escaner_no_se_queja_de_lo_correcto`, `todo_lo_que_hay_en_web_esta_incrustado_y_escaneado`, `el_texto_del_registro_no_puede_inyectar_marcado` |

## Qué se midió

- **Redacción.** Se probaron:
  - 56 formas sensibles, también en mayúsculas y en minúsculas;
  - 6 valores separados de su clave por un salto de línea;
  - 17 líneas sin nada sensible, que no cambian;
  - 2000 textos generados con semilla fija, de 1 a 5 secretos cada uno, entre 15 formas y con
    separadores al azar.

  Ningún secreto escapó, redactar otra vez no cambió nada y la cantidad de líneas se mantuvo.
- **Cola.** De un registro de 4 MiB se leen exactamente 65 KiB: la ventana de 64 KiB más 1 KiB
  de contexto. En un registro de unos 3 KiB, para cada uno de los más de 3000 puntos de corte
  posibles, la cola redactada coincide línea por línea con la del archivo leído entero.
- **Servidor.** Con un plazo de 600 ms, un cliente que gotea un byte cada 50 ms recibe 408 y su
  lugar se libera aunque siga goteando. Las pruebas que dependen del tiempo corrieron 8 veces
  seguidas sin fallar.
- **Interfaz.** Se revisó en Chromium sin ventana, en claro y oscuro, a 1280 y a 390 px de ancho:
  ninguna violación de la CSP, ningún error de la página, ningún desborde horizontal (ni en la
  página ni dentro de las tablas). El recorrido con Tab sigue el orden visual y el foco se ve.
- **Huérfanos.** Con la demostración corriendo con 6 trabajadores, `kill -9` a la consola: a
  los 5 s no quedaba ningún trabajador vivo. Con el binario y 4 trabajadores, `SIGTERM`: igual.

## Correr

```bash
cargo test                 # en Linux: 91 pruebas y 5 doctests
cargo run --example demo   # http://127.0.0.1:8750/ con trabajadores de verdad y datos inventados
```

La demostración usa este mismo ejecutable como trabajador, inventa los servicios
(`miniaturas-*`) y escribe un registro de mentira, lleno de secretos falsos, que crece con una
línea cada dos segundos. Cada tanto un trabajador se cae solo, para que se vea al tick
reemplazarlo.

El binario, con un trabajador y un registro de verdad:

```bash
cargo build
target/debug/ops-console --escuchar 127.0.0.1:8750 \
    --trabajador target/debug/trabajador-demo \
    --registros servicio.log --min 2 --max 8 \
    --servicio miniaturas-api.service
```

```
$ ops-console --escuchar 0.0.0.0:8750 --trabajador … --registros …
ops-console: no escucho en 0.0.0.0:8750: 0.0.0.0 no es de esta máquina: la consola solo
escucha en loopback (127.0.0.0/8 o ::1)
```

## La API

| Ruta | Método | Qué hace |
|---|---|---|
| `/` (y `/app.css`, `/app.js`, `/icono.svg`) | GET | la interfaz |
| `/api/estado` | GET | la consola y el grupo: política, posiciones y última revisión |
| `/api/grupo/reciclar` | POST | `{"posicion": id}` |
| `/api/grupo/escala` | POST | `{"cantidad": n}`, `{"min": a, "max": b}` o los tres juntos (todo o nada) |
| `/api/grupo/tick` | POST | `{}`: revisa el grupo ahora |
| `/api/servicios` | GET | el estado de cada unidad vigilada |
| `/api/registros?n=…` | GET | las últimas `n` líneas (1 a 500) del registro, redactadas |
| `/api/sistema` | GET | si cada variable de configuración está definida |

Toda respuesta es `{"ok": true, "datos": …}` o `{"ok": false, "error": {"estado", "codigo",
"mensaje"}}`. Un cuerpo con un campo que la ruta no espera se rechaza entero.

## Plataforma

El servidor, el grupo y la interfaz son para Linux y Windows. Las pruebas que miran o matan un
proceso desde afuera usan `kill` o `tasklist`/`taskkill` según el sistema. El vigía consulta
`systemctl`, que solo existe en Linux: en Windows la sección Servicios muestra «no disponible»
con el motivo. Se compiló y se probó en Linux; en Windows no se probó.

## Límites conocidos

- **Loopback no es autenticación.** Las defensas protegen del navegador, no de los procesos de
  la máquina: cualquier programa local puede mandar el `Host`, el `Origin` y la cabecera propia
  que quiera. En una máquina compartida, la consola es de todos sus usuarios.
- **HTTP mínimo.** No hay TLS (no sale de la máquina), ni keep-alive, ni *chunked*, ni `HEAD`.
  Hay un hilo por conexión, con un tope de 32 conexiones a la vez.
- **Un `POST` sin `Content-Length` es 411, no 413.** Sin *chunked*, no hay forma de leer un
  cuerpo que no anuncia su largo.
- **La redacción es por patrones.** Tapa las formas descritas, no cualquier secreto.
  - Un valor sin comillas después de `=` se tapa hasta el primer espacio.
  - Un valor entre comillas que sigue en otra línea se tapa solo hasta el final de la primera.
  - No se reconocen un bloque PEM ni un token suelto sin clave ni prefijo conocido.
  - Una clave y su valor separados por más de 256 bytes ya no se consideran ligados.
  - También tapa de más: en `token: expirado`, se pierde «expirado». Y `PWD=/ruta`, la variable
    del directorio actual, también se tapa: es una sobrerredacción asumida, porque `pwd` es
    además la abreviatura común de una contraseña.
- **La consola supervisa; no reparte trabajo.** Lanza, recicla y reemplaza procesos, pero no
  les manda tareas. `trabajador-demo` atiende tareas por su entrada estándar para mostrar el
  protocolo.
- **Ctrl-C no pasa por `Drop`.** Sin un manejador de señales, el grupo no llega a terminar sus
  procesos. Los trabajadores terminan solos porque se cierra su entrada: `trabajador-demo` lo
  hace, y otro programa tendría que hacer lo mismo.
- **La interfaz se revisó en Chromium sin ventana.** No se probó en Firefox ni en Safari, ni
  con un lector de pantalla.

## Licencia

MIT — ver [LICENSE](LICENSE).
