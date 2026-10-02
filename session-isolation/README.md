# session-isolation

**Aislamiento de sesiones por proceso, en Rust: una instantánea compartida lock-free
y sin `unsafe`, un proceso efímero por petición, y un pool que reemplaza procesos
en caliente sin perder el slot.**

Tres piezas que resuelven un problema común de los servicios que ejecutan trabajo no
confiable o inestable: que un trabajo que se cuelga, se come la memoria o inunda su
salida no arrastre al resto.

```
  escritor ──publicar()──▶ ┌──────────────────────┐ ◀──leer()── N lectores
                           │ Publicador (seqlock) │   sin locks, sin unsafe
                           └──────────┬───────────┘
                                      │ copia de la instantánea
                                      ▼
              ipc::ejecutar ──stdin──▶ [proceso efímero] ──stdout──▶ respuesta
                             plazo · tope de salida · siempre se espera al hijo

  Pool ─ slots con identidad estable ─ reemplazo en caliente ─ Juez con histéresis
```

---

## Qué demuestra

### 1 · Un seqlock correcto, en safe Rust

El seqlock clásico guarda el valor en una celda normal y lo lee mientras el escritor
puede estar escribiendo; si la secuencia cambió, descarta la copia. El problema es que
esa lectura concurrente **ya es una carrera de datos**: comportamiento indefinido en el
modelo de memoria de Rust, aunque en x86 "funcione".

Acá cada palabra de la carga útil es un `AtomicU64`, leído y escrito con orden `Relaxed`
entre *fences* de adquisición y liberación — el protocolo que describe Hans Boehm en
*Can seqlocks get along with programming language memory models?*. Leer una palabra a
medio actualizar está definido, y la secuencia dice si hay que descartarla. El crate
entero lleva `#![forbid(unsafe_code)]`.

Dos detalles más: un mutex serializa a los escritores (dos a la vez romperían la
paridad de la secuencia), y el valor se codifica **antes** de abrir la ventana de
escritura, para que un pánico en código del usuario no deje la secuencia impar y a los
lectores esperando para siempre.

**El test tiene dientes.** El test de estrés corre 200 000 escrituras contra 4 lectores
y exige que ninguna lectura mezcle dos escrituras. Con el lector roto a propósito (sin
verificar la secuencia), falló en 3 de 3 corridas y atrapó lecturas como
`[2522, 2522, 2521, 2521]`: mitad de una escritura, mitad de otra.

### 2 · Sesiones efímeras: un proceso por petición

`ipc::ejecutar` lanza un proceso, le pasa la petición por stdin como JSON y lee la
respuesta por stdout. El hijo solo ve lo que se le manda: nunca la memoria del
coordinador. Y se contienen las tres formas de portarse mal:

| El proceso… | Qué pasa |
|---|---|
| se cuelga | al vencer el plazo se lo mata **y se lo espera** — no quedan zombis |
| inunda la salida | al pasar el tope de bytes se lo mata, sin esperar a que termine de escribir |
| no lee su entrada | stdin se escribe desde otro hilo: no puede bloquear al coordinador |
| cierra stdout y sigue vivo | se le da lo que queda del plazo, y si no termina, se lo mata |
| falla | se devuelve su código de salida y su stderr (recortado) |

### 3 · Pool con reemplazo en caliente

Cada slot tiene una **identidad estable** — id, etiqueta, receta, fecha de creación — y
un proceso que la encarna. El proceso es desechable; el slot no.

El reemplazo lanza el proceso nuevo **antes** de terminar el viejo. La capacidad nunca
cae durante el cambio, y si el nuevo no arranca, el viejo sigue atendiendo: un
reemplazo fallido no puede dejar un slot vacío. Eso necesita un proceso de más durante
el cambio, que el pool reserva como **margen de reemplazo** — la misma idea que
`maxSurge` en las actualizaciones continuas de Kubernetes. Y como en Kubernetes, se
puede cambiar la receta slot por slot (`reemplazar_con_receta`).

### 4 · Un juez con histéresis

Reemplazar tiene costo, así que un pico aislado no alcanza: un slot se reemplaza tras
`persistencia` mediciones críticas **consecutivas**. Una medición sana reinicia la
cuenta; una de aviso la deja como está. Y **un dato ausente no es un dato sano**: si no
se pudo medir nada, la cuenta no se toca.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Ninguna lectura mezcla dos escrituras (200 000 escrituras, 4 lectores) | `instantanea::ninguna_lectura_mezcla_dos_escrituras` |
| Varios escritores no rompen la secuencia | `instantanea::varios_escritores_no_rompen_la_secuencia` |
| La sesión trabaja sobre su copia, no sobre lo publicado después | `una_sesion_trabaja_sobre_la_instantanea_que_recibio` |
| Un proceso colgado se termina y **no queda zombi** (Linux: desaparece de `/proc`) | `un_proceso_colgado_se_termina_y_se_cosecha` |
| Una salida desbordada se corta | `una_salida_desbordada_se_corta` |
| El reemplazo conserva la identidad del slot y cosecha al viejo | `reemplazar_cambia_el_proceso_y_conserva_la_identidad_del_slot` |
| El nuevo arranca antes que termine el viejo (sin margen, se rechaza) | `sin_margen_el_reemplazo_se_rechaza_y_la_victima_sigue` |
| Un reemplazo que no arranca deja el slot intacto, receta incluida | `si_el_reemplazo_no_arranca_el_slot_queda_intacto` |
| Solo se reemplaza al slot enfermo, y no por un pico | `el_ciclo_de_salud_reemplaza_solo_al_slot_enfermo`, `salud::tests::*` |
| Un proceso caído se detecta y se reemplaza | `un_proceso_caido_se_reemplaza_en_el_siguiente_ciclo` |
| Soltar el pool termina y cosecha todos sus procesos | `soltar_el_pool_termina_y_cosecha_todos_sus_procesos` |

Los tests usan un trabajador real (`trabajador-demo`), con tareas que se portan mal a
propósito: colgarse, inundar la salida, fallar.

## Correr

```bash
cargo test                                    # 24 tests
cargo build --bins && cargo run --example demo
```

```
2 · Sesiones efímeras (un proceso por petición)

   escalar [1, 2, 3]      → [10, 20, 30] (generación 1)
   colgarse 10 s          → contenido: el proceso 2819412 no terminó en 500ms
   inundar 1 MiB          → contenido: el proceso escribió más de 4096 bytes

3 · Pool con reemplazo en caliente

   ciclo 3: slot 2 · pid 2819425 → 2819427 · métricas críticas sostenidas
```

## Límites conocidos

- **El seqlock favorece a los escritores.** Un escritor que publica sin pausa puede
  hacer reintentar a los lectores indefinidamente. Es para estado de escritura rara y
  lectura masiva — configuración, tablas de enrutamiento —, no para contadores calientes.
- **La medición real de memoria es solo de Linux** (`/proc/<pid>/status`). En otros
  sistemas devuelve `None`, y el juez lo trata como "sin datos", nunca como sano.
- **Un proceso por sesión tiene costo de arranque.** Es el precio del aislamiento; para
  trabajo muy corto y confiable, un hilo es más barato.
- Las pruebas de "no quedan zombis" verifican `/proc`, así que corren solo en Linux; el
  resto corre en cualquier sistema.

## Licencia

MIT — ver [LICENSE](LICENSE).
