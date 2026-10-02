# sealed-checkpoint

**Checkpoints de estado sellados con XChaCha20-Poly1305: escritura atómica que sobrevive a
un corte de luz, encabezado autenticado, sellos ligados a su nombre y protección contra
retroceso.**

Guardar estado cifrado parece resuelto con "cifrá y escribí el archivo". No lo está. Un
atacante con acceso al disco puede **alterar** un byte, **renombrar** un archivo para que
pase por otro, o **reponer una copia vieja** que sigue siendo auténtica. Y un corte de luz
en el momento equivocado puede dejar el archivo a medias, o perder el cambio aunque los
bytes ya estuvieran escritos. Este crate cubre los cinco casos.

```rust
let almacen = Almacen::abrir("estado/", Clave::generar()?)?;
almacen.guardar("sesion", 7, &bytes)?;              // la generación solo avanza

let ultimo = almacen.cargar_desde("sesion", 7)?;     // rechaza una copia anterior a la 7
```

---

## Qué demuestra

- **Cifrado autenticado con los datos correctos asociados.** El encabezado (formato,
  versión, generación) va en claro — la generación se puede leer sin la clave — pero
  **autenticado** como datos asociados (AAD): cambiar un solo bit hace fallar la apertura.
- **Un sello ligado a su nombre.** El nombre del checkpoint entra en el AAD. Un archivo de
  `sesion` copiado como `config` no abre.
- **Protección contra retroceso.** Un sello viejo sigue siendo auténtico, así que el
  cifrado no alcanza. `guardar` rechaza una generación que no avanza, y `cargar_desde`
  rechaza una anterior a la mínima que el llamador ya conoce.
- **Escritura atómica y durable, completa.** Temporal nuevo (`create_new`, nunca sigue un
  enlace preexistente) → `sync_all` → `rename` atómico → **`sync_all` del directorio**.
  Ese último paso es el que suele faltar: sin él, el `rename` puede perderse ante un corte
  aunque los bytes ya estén en disco.
- **Nonces de 192 bits.** XChaCha20-Poly1305 en vez de ChaCha20-Poly1305: con nonces
  aleatorios de 96 bits el límite seguro ronda los 2³² sellos por clave; con 192 deja de
  ser una preocupación práctica.
- **Secretos que no quedan en memoria ni en los logs.** La clave y los datos abiertos se
  borran al soltarse (`zeroize`), y su `Debug` no los muestra.
- **Un solo error para todo lo que no abre.** Clave equivocada, nombre equivocado o datos
  alterados dan el mismo error: no se le da a un atacante una pista sobre cuál fue.
- **Cero `unsafe`.**

## Formato

```
0      4   magia "SCKP"
4      1   versión del formato (1)
5      8   generación, u64 little-endian       ─┐ encabezado: en claro,
                                                ─┘ autenticado como AAD
13     24  nonce aleatorio
37     n   datos cifrados
37+n   16  etiqueta de autenticación

AAD = encabezado ‖ largo(nombre) ‖ nombre
```

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| **Cualquier** bit alterado — de cualquier byte — hace fallar la apertura | `cualquier_bit_alterado_hace_fallar_la_apertura` (exhaustivo) |
| La generación se lee sin clave, pero no se puede alterar | `alterar_la_generacion_del_encabezado_no_pasa_desapercibido` |
| Un checkpoint renombrado no abre | `un_checkpoint_renombrado_no_abre_con_otro_nombre` |
| Una copia vieja repuesta se detecta con la generación mínima | `reemplazar_el_archivo_por_una_copia_vieja_se_detecta_con_la_minima` |
| Guardar una generación que no avanza se rechaza | `una_generacion_que_no_avanza_se_rechaza_al_guardar` |
| No quedan temporales, y uno huérfano de una caída no molesta | `no_quedan_temporales_despues_de_guardar`, `un_temporal_huerfano_de_una_caida_no_afecta_la_carga` |
| Un archivo corrupto da error, nunca datos basura, y se puede reemplazar | `un_archivo_corrupto_da_error_y_se_puede_reemplazar` |
| Los nombres no pueden escapar del directorio | `los_nombres_que_podrian_escapar_del_directorio_se_rechazan` |
| Ninguna entrada, de ningún largo, provoca un pánico | `ningun_largo_de_entrada_provoca_panico` |
| El `Debug` no muestra claves ni datos | `el_debug_no_muestra_secretos` |

## Correr

```bash
cargo test                  # 18 tests
cargo run --example demo    # guardar, cargar y tres ataques detectados
```

```
4 · Alguien repone una copia vieja (auténtica)

   cargar              → ok, generación 1
   cargar_desde(mín 2) → error: el checkpoint retrocedió: generación 1, la mínima conocida es 2
```

## Límites conocidos

- **Un escritor por nombre.** La verificación de que la generación avanza y la escritura no
  son una sola operación atómica: con varios procesos escribiendo el mismo checkpoint, hay
  que serializarlos afuera.
- **La generación mínima la guarda el llamador.** El almacén no puede detectar por sí solo
  una copia vieja repuesta: necesita saber cuál fue la última que se vio (en memoria, en
  otro almacén, en un contador de hardware).
- **La durabilidad del directorio es de Unix.** La biblioteca estándar no permite
  sincronizar un directorio en Windows; ahí el `rename` es atómico pero su durabilidad
  depende del sistema de archivos.
- **No deriva claves de contraseñas.** La clave llega hecha; derivarla bien (Argon2,
  scrypt) es otro problema, fuera del alcance.

## Licencia

MIT — ver [LICENSE](LICENSE).
