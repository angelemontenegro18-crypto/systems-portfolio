# state-handoff

**Traspaso de estado entre un controlador activo y su respaldo, en `no_std`: un paquete con
dos CRC-32, una cerca de época que rechaza repeticiones, verificación por tipos y una
confirmación en dos fases que borra lo que deja atrás.**

Una bomba tiene dos controladores: el activo y su respaldo. Cuando el activo ajusta la tabla
de calibración, el respaldo tiene que tenerla igual, para tomar el control sin saltos. Pasarla
parece trivial y tiene cuatro trampas: que llegue alterada, que llegue una copia vieja (o la
misma dos veces) y se aplique, que se lea antes de verificarla, y que quede una copia
olvidada en un búfer. Este crate cierra las cuatro, sin `alloc`, en memoria compartida o por
un enlace: el paquete es una tira de bytes que se explica sola.

```rust
let n = sellar(&tabla, ACTIVO, RESPALDO, 7, &mut bytes)?;   // cabecera + dos CRC-32

respaldo.cargar(&bytes[..n])?;
let preparado = respaldo.preparar()?;      // verifica, y la época tiene que ser > la última
bomba.instalar(preparado.contenido())?;    // si falla, soltar el preparado es abortar
preparado.confirmar();                     // registra la época 7 y borra el área
```

---

## Qué demuestra

- **Un formato con el largo atado a sus campos, en compilación.** La cabecera se declara una
  sola vez, como lista de campos; de ahí salen el `struct`, la escritura y la lectura en
  little-endian, y una aserción constante: el largo declarado es la suma de los anchos.
  Agregar un campo sin actualizar el largo no compila (doctest `compile_fail,E0080`, junto a
  su versión corregida, que sí compila).
- **CRC-32 escrito a mano, con la tabla calculada en compilación.** El de IEEE 802.3 (el de
  zlib). La tabla se recorre con `split_first_mut`, sin indexar ni siquiera en `const`. Se
  comprueba contra el valor de la norma (`"123456789"` → `0xCBF43926`), contra valores de
  `zlib` y contra una implementación bit a bit, sin tabla, escrita en las pruebas.
- **Todo error de uno y de dos bits se detecta.** En un paquete de 36 bytes, los 288 errores
  simples y los 41 328 dobles, uno por uno; y lo truncado en cualquier largo, lo extendido, la
  firma y la versión ajenas (aunque su CRC cierre) y el destino equivocado.
- **No se lee lo que no se verificó.** `Paquete<Sellado>` no tiene método para leer el
  contenido (doctest `compile_fail,E0599`); solo `verificar` lo convierte en
  `Paquete<Verificado>`, que no se puede fabricar de otra forma (`compile_fail,E0451`).
- **Una cerca de época.** El respaldo solo acepta una época estrictamente mayor que la última
  que confirmó: una repetición o un estado viejo se rechazan. Tres secuencias pseudoaleatorias
  de 2000 paquetes —nuevos, repetidos y viejos, con abortos de por medio— se cotejan paquete a
  paquete contra el modelo de una línea, y las épocas aceptadas crecen estrictamente.
- **Confirmación en dos fases.** `preparar` verifica y pasa la cerca; `confirmar` registra la
  época; `abortar` —o soltar el preparado— no la gasta, y el mismo paquete se puede volver a
  preparar. Confirmar consume el preparado: no se confirma dos veces (`compile_fail,E0382`).
  Mientras hay un preparado, el receptor está prestado y no se le puede cargar otro paquete
  encima (`compile_fail,E0499`).
- **Lo que se confirma, se borra.** Al confirmar, el área de preparación se borra con
  `zeroize`, que el compilador no puede eliminar por inútil, y el borrado se ve desde la API.
  Cargar un paquete más corto borra lo que quedaba del anterior.
- **Little-endian explícito, sin `unwrap` ni indexación.** Un escritor y un lector con cursor,
  donde todo acceso pasa por `get`/`get_mut` y lo que no cabe o falta es un error que no mueve
  el cursor.
- **`no_std` de verdad.** Sin `alloc`; la única dependencia es `zeroize`, sin sus
  características por defecto (que traerían `alloc`). `#![forbid(unsafe_code)]` y `#![deny]` de
  `unwrap`, `expect`, `panic!` e indexación; una prueba escanea el código fuente por lo mismo y
  comprueba el manifiesto.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| El largo de la cabecera es la suma de sus campos, en compilación | doctests de `formato_de_cabecera!` (`compile_fail,E0080` y su versión corregida) |
| La cabecera tiene la forma documentada, byte a byte | `la_cabecera_tiene_la_forma_documentada_byte_a_byte` (posiciones escritas a mano) |
| CRC-32 según la norma, `zlib` y una implementación bit a bit | `el_valor_de_verificacion_de_la_norma`, `otros_valores_conocidos`, `coincide_con_la_referencia_bit_a_bit` |
| Todo error de uno o de dos bits se detecta | `todo_error_de_un_bit_se_detecta`, `todo_error_de_dos_bits_se_detecta` |
| Con la cabecera intacta, el CRC del contenido es el que atrapa | `con_la_cabecera_intacta_el_crc_del_contenido_es_el_que_atrapa` |
| Truncado en cualquier largo, extendido o con un largo mentiroso: rechazado | `un_paquete_truncado_se_rechaza_en_cualquier_largo`, `un_paquete_extendido_se_rechaza`, `un_largo_mentiroso_con_crc_que_cierra_se_rechaza` |
| Firma, versión o destino ajenos: rechazado, aunque el CRC cierre | `una_firma_ajena_se_rechaza_aunque_su_crc_cierre`, `una_version_desconocida_se_rechaza_aunque_su_crc_cierre`, `un_paquete_para_otro_nodo_se_rechaza` |
| El contenido no se lee sin verificar, y un verificado no se fabrica | doctests `compile_fail,E0599` y `compile_fail,E0451` de `paquete` |
| Solo pasa una época estrictamente mayor que la última confirmada | `una_repeticion_o_una_epoca_vieja_se_rechazan`, `la_epoca_cero_no_pasa_nunca`, `con_secuencias_pseudoaleatorias_las_epocas_aceptadas_crecen_estrictamente` |
| Abortar, o soltar el preparado, no gasta la época | `abortar_no_consume_la_epoca`, `soltar_el_preparado_sin_confirmar_es_abortar` |
| Un paquete rechazado no toca la época | `un_error_de_verificacion_no_toca_la_epoca` |
| Confirmar borra el área de preparación | `confirmar_borra_el_area_de_preparacion` |
| No se confirma dos veces, ni se carga encima de un preparado | doctests `compile_fail,E0382` y `compile_fail,E0499` de `receptor` |
| Cargar no deja restos de la carga anterior; lo que no cabe no toca el área | `cargar_borra_lo_que_quedaba_de_una_carga_anterior`, `cargar_lo_que_no_cabe_es_un_error_y_no_toca_el_area` |
| La cerca se restaura después de un reinicio | `un_receptor_reiniciado_recuerda_su_cerca` |
| Little-endian exacto; lo que falta o no cabe no mueve el cursor | `tests/le.rs` (4 pruebas) |
| Sin `std`, sin `alloc`, sin `unwrap`, sin indexación; solo `zeroize`, sin `alloc` | `tests/sin_std.rs` (4 pruebas) y los `#![deny(clippy::…)]` de `lib.rs` |

## Qué se midió

- **Detección de errores:** de 41 616 paquetes con uno o dos bits volteados (sobre 36 bytes),
  se rechazan los 41 616. Los 48 errores de un bit en el contenido llegan todos al CRC del
  contenido.
- **Cerca de época:** con las semillas 1, 2 y 3, 2000 paquetes cada una: 191, 189 y 190
  aceptados; 1707, 1712 y 1735 rechazados por la cerca; 102, 99 y 75 abortados. El receptor
  coincide con el modelo en los 6000 paquetes. Las cifras las imprime
  `cargo test --test receptor -- --nocapture`.

## Correr

```bash
cargo test                       # 35 pruebas y 8 doctests
cargo run --example traspaso     # los casos de un traspaso, contados
```

```
paquete de la época 41: 62 bytes
traspaso en memoria: confirmada la época 41 (10 Hz → 81 L/min); área en ceros: true
repetición de la 41: rechazado (época 41 no posterior a la última confirmada (41))
la 42 con un bit volteado: rechazado (el CRC del contenido no coincide)
la 42 retransmitida: preparada; la bomba está ocupada, se aborta
última época confirmada: 41
la 42, cuando la bomba se libera: confirmada la época 42 (10 Hz → 82 L/min); área en ceros: true
la 43 para el nodo 7: rechazado (el paquete es para el nodo 7)
última época confirmada: 42
```

## Límites conocidos

- **El CRC detecta accidentes, no ataques.** Quien cambie el contenido puede recalcular los
  dos CRC: no hay autenticación. Contra alguien que escribe en el enlace hace falta un código
  de autenticación de mensajes con clave.
- **La cerca vive en memoria.** Si el respaldo se reinicia, la última época la tiene que
  guardar la aplicación y devolverla con `Receptor::con_epoca`; el crate no persiste nada.
- **Las épocas las elige quien sella.** El crate rechaza lo que no crece, pero no lleva el
  contador del activo. Las épocas válidas empiezan en 1, y después de `u64::MAX` no pasa nada
  más.
- **Un paquete entero por vez.** `cargar` copia un paquete completo al área de preparación;
  rearmar un paquete que llega en trozos es trabajo del enlace.
- **El borrado alcanza al área del receptor, y a nada más.** No borra las copias que haya
  hecho la aplicación, ni el búfer del activo, ni registros o cachés del procesador. La prueba
  ve los ceros por la API; no puede ver la memoria por fuera de ella.
- **Uno y dos bits, garantizado; más, probable.** La detección exhaustiva está hecha en un
  paquete de 36 bytes. Con errores más grandes, el CRC-32 deja pasar del orden de uno en
  2³² errores al azar.
- **Una sola versión del formato.** Los nodos no acuerdan otra: una versión distinta de 1 se
  rechaza.

## Licencia

MIT — ver [LICENSE](LICENSE).
