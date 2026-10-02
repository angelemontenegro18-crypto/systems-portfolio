# tool-gateway

**Registro de herramientas para modelos de lenguaje con *function calling*: solo funciones
puras, una lista de exclusión verificada por test, y argumentos validados antes de llamar.**

Un agente con herramientas es tan seguro como la herramienta más peligrosa que puede
llamar. Este crate aplica una regla simple y la vuelve verificable: **solo se exponen
funciones puras** — sin E/S, sin estado, sin aleatoriedad, sin efectos.

```rust
let registro = tool_gateway::catalogo()?;
let manifiesto = registro.manifiesto();          // se le pasa al modelo

// El modelo responde con una llamada:
let resultado = registro.responder("llamada_1", "distancia",
    &json!({"a": {"x": 0, "y": 0}, "b": {"x": 3, "y": 4}}));
// → {"tool_use_id": "llamada_1", "content": "{\"distancia\":5.0}", "is_error": false}
```

---

## Qué demuestra

- **Sin estado por construcción.** Cada herramienta es un puntero a función (`fn`), no un
  closure. Un `fn` no puede capturar estado: "sin estado" lo garantiza el tipo, no la
  disciplina de quien registra.
- **Una lista de exclusión que se hace cumplir.** Categorías enteras quedan vetadas —
  lanzar procesos, leer o escribir disco, red, correo, variables de entorno, azar, hora —
  y el registro **se niega** a aceptarlas. Un test recorre la lista y lo verifica nombre
  por nombre.
- **Argumentos validados antes de llamar.** Los modelos producen argumentos que parecen
  correctos y no lo son. Se validan contra un subconjunto de JSON Schema y el error dice
  exactamente dónde (`/valores/1: se esperaba number y llegó string`), con **todos** los
  problemas a la vez, para que el modelo corrija en un intento.
- **Un esquema no puede prometer de más.** El validador es un subconjunto cerrado: un
  esquema que usa una palabra clave que no se aplica (`pattern`, por ejemplo) se rechaza al
  registrar. Nadie cree tener una restricción que no existe.
- **Nada tumba al gateway.** Una herramienta que falla o que entra en pánico vuelve al
  modelo como resultado con `is_error: true`. Hay límites de tamaño para la entrada y la
  salida.
- **Cada herramienta trae un ejemplo** que tiene que cumplir su propio esquema, y un test
  llama cada herramienta dos veces con su ejemplo para verificar que es determinista.
- **Cero `unsafe`**, una dependencia (`serde_json`).

## La política de exposición

| Vetada | Por qué |
|---|---|
| `ejecutar_comando` | Lanza procesos: un efecto fuera del gateway y sin límite |
| `leer_archivo` | E/S de disco: puede exfiltrar cualquier archivo legible |
| `escribir_archivo` | E/S de disco con efectos persistentes |
| `consultar_url` | E/S de red: abre la puerta a SSRF y a exfiltración |
| `enviar_correo` | Efecto externo e irreversible en nombre del usuario |
| `leer_variable_entorno` | El entorno suele guardar credenciales |
| `numero_aleatorio` | No determinista: la misma llamada da resultados distintos |
| `hora_actual` | No determinista, y filtra información del entorno |

Los nombres son ejemplos de las categorías de riesgo típicas de un agente con
herramientas. La lista vive en `src/politica.rs`.

## El catálogo de ejemplo

Seis funciones puras: `crc32`, `estadisticas`, `convertir_temperatura`, `validar_isbn`,
`contar_palabras` y `distancia`. Muestran la forma de una herramienta bien declarada:
nombre, descripción que el modelo pueda usar para decidir, esquema cerrado
(`additionalProperties: false`) y un ejemplo válido.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Ninguna herramienta vetada está en el catálogo | `ninguna_herramienta_vetada_esta_en_el_catalogo` |
| El registro rechaza cada nombre vetado, con su motivo | `el_registro_se_niega_a_aceptar_cada_herramienta_vetada` |
| Cada ejemplo cumple su esquema | `cada_herramienta_tiene_un_ejemplo_que_cumple_su_esquema` |
| Cada herramienta es determinista | `cada_herramienta_es_determinista` |
| Un esquema con restricciones no soportadas se rechaza | `nombres_duplicados_invalidos_y_esquemas_que_prometen_de_mas_se_rechazan` |
| Un pánico dentro de una herramienta no tumba el gateway | `un_panico_de_la_herramienta_no_tumba_el_gateway` |
| Entrada y salida tienen tope de tamaño | `los_limites_de_tamano_se_aplican_a_la_entrada_y_a_la_salida` |
| Los errores vuelven al modelo con la marca de error | `responder_arma_el_resultado_para_el_modelo_con_la_marca_de_error` |

Los valores esperados de los tests de cálculo (CRC-32, estadísticas, ISBN) se obtuvieron
con una referencia independiente, no con este mismo código.

## Correr

```bash
cargo test                  # 22 tests
cargo run --example demo    # un modelo simulado haciendo llamadas buenas y malas
```

```
  [ok   ] distancia              → {"distancia":5.0}
  [error] estadisticas           → argumentos inválidos: /valores/1: se esperaba number y llegó string
  [error] leer_archivo           → no existe la herramienta `leer_archivo`

Intento de registrar una herramienta vetada:
  rechazada: `ejecutar_comando` está vetada: lanza procesos: un efecto fuera del gateway y sin límite
```

## Límites conocidos

- **La pureza no se puede probar desde afuera.** El tipo `fn` descarta el estado capturado,
  y el test de determinismo atrapa el caso típico (hora, azar, contador global), pero una
  herramienta podría usar un `static` con mutabilidad interior. La lista de exclusión y la
  revisión de código siguen siendo necesarias.
- **El validador es un subconjunto de JSON Schema**, a propósito: tipos, `properties`,
  `required`, `additionalProperties: false`, `items`, rangos, largos y `enum`. Lo que no
  soporta, lo rechaza.
- **El determinismo se prueba con una entrada por herramienta** (su ejemplo), no con todas.

## Licencia

MIT — ver [LICENSE](LICENSE).
