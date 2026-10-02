# modbus-ro

**Cliente Modbus TCP de solo lectura, en Rust. La escritura no está prohibida: no existe.**

Para paneles que monitorean equipos industriales — PLCs, variadores, medidores —
donde un bug o un clic equivocado no debería poder cambiar un setpoint. Casi todas
las librerías Modbus exponen lectura y escritura con la misma API y confían en que
quien la usa no se equivoque. Esta no te deja equivocarte: la operación de escritura
no se puede expresar.

```rust
let mut cliente = Cliente::conectar(direccion, 1, Duration::from_secs(2), Duration::from_secs(1))?;
let valores = cliente.leer(Funcion::RegistrosRetencion, 0, 4)?;
```

---

## Qué demuestra

- **Seguridad por construcción, no por disciplina.** `Funcion` tiene exactamente dos
  variantes, las dos de lectura. No hay API que acepte un código de función arbitrario
  ni que envíe un PDU crudo.
- **Un test que prueba lo que el código *no contiene*.** Recorre el propio `src/` y falla
  si aparece un código de función de escritura o el nombre de una operación de escritura.
  Un segundo test verifica que ese escáner de verdad detectaría una violación.
- **Validación estricta del protocolo.** Una respuesta de otra transacción, de otra
  unidad, con otro largo o con un conteo de bytes que no cuadra se rechaza con un error
  que dice exactamente qué falló — nunca se devuelven valores que "parecen" buenos.
- **Diseño para paneles.** El sondeador lee en su propio hilo y sirve la última lectura
  desde un caché. El lock del caché nunca se toma durante una operación de red, así que
  un equipo que no contesta no congela la interfaz.
- **Cero dependencias, cero `unsafe`** (`#![forbid(unsafe_code)]`).

## Garantías y cómo se prueban

| Garantía | Cómo se sostiene | Test |
|---|---|---|
| No se puede escribir en un equipo | `Funcion` solo tiene variantes de lectura | `solo_lectura::el_codigo_fuente_no_contiene_ningun_camino_de_escritura` |
| Ninguna dependencia aporta un camino de escritura | `[dependencies]` vacío | `solo_lectura::el_crate_no_declara_dependencias` |
| El escáner no es decorativo | se le da código con una violación y la marca | `solo_lectura::el_escaneo_detectaria_una_violacion` |
| Una respuesta ajena nunca pasa como propia | transacción, unidad, función y largos se validan contra la petición | `trama::tests::cada_desajuste_tiene_su_propio_error`, `protocolo::una_respuesta_de_otra_transaccion_se_rechaza` |
| Un equipo mudo no cuelga al cliente | timeouts de conexión y de petición | `protocolo::un_equipo_mudo_produce_timeout_y_no_cuelga` |
| Consultar el caché nunca espera a la red | la lectura se arma completa antes de tomar el lock | `protocolo::el_sondeador_sirve_el_cache_sin_esperar_a_la_red` |
| Una excepción Modbus no desincroniza la conexión | solo las excepciones conservan el flujo; cualquier otra falla lo reabre | `protocolo::una_excepcion_del_equipo_llega_como_error_con_nombre` |

Los tests de protocolo corren contra un equipo simulado en el mismo proceso, que
decodifica las peticiones **por su cuenta**, sin usar el códec del crate: un error
simétrico — codificar y decodificar mal de la misma forma — no pasaría inadvertido.

## Diseño

```
                    lecturas()  (copia inmediata)
   panel  ──────────────────────────────▶  ┌──────────┐
                                           │  caché   │  Mutex: se toma solo para
                                           └────▲─────┘  copiar o reemplazar
                                                │ publica la lectura ya completa
                                        ┌───────┴────────┐
                                        │ hilo de sondeo │── TCP ──▶ equipo A
                                        │  (reconecta)   │── TCP ──▶ equipo B
                                        └────────────────┘
```

| Módulo | Responsabilidad |
|---|---|
| `trama` | Codificar peticiones de lectura y validar respuestas (MBAP + PDU). Sin E/S. |
| `cliente` | Hablar con una unidad sobre cualquier `Read + Write`, leyendo exactamente lo que la cabecera anuncia. |
| `sondeo` | Leer varios equipos en segundo plano, reconectar tras fallas y servir el último estado. |

Cada equipo queda en uno de cuatro estados: `Sondeando` (todavía no terminó el primer
ciclo), `EnLinea`, `Degradado` (algunos bloques se leyeron y otros no) o `FueraDeLinea`.

## Correr

```bash
cargo test                  # 17 tests: trama, protocolo contra el simulador, escaneo de fuente
cargo run --example demo    # levanta un equipo simulado y lo lee
```

La demo no necesita hardware: levanta un equipo Modbus de juguete en el mismo proceso.

## Límites conocidos

- **El escaneo de fuente es una red, no el muro.** Busca códigos de escritura como
  literales hexadecimales y nombres de operación conocidos; un código escrito en decimal
  no lo detectaría. La garantía de fondo es el tipo: no existe una API que acepte un
  código de función.
- **Solo registros de 16 bits** (funciones 3 y 4). Bobinas y entradas discretas
  (funciones 1 y 2) también son de lectura y se podrían agregar sin tocar la garantía.
- **Síncrono.** Un hilo para el sondeo; no hay versión `async`.
- **Detener el sondeador** puede tardar hasta que venza el timeout de la lectura en curso.

## Licencia

MIT — ver [LICENSE](LICENSE).
