# content-triage

**Triaje determinista de contenido web no confiable, antes de que llegue a un modelo de lenguaje.**

Un agente que lee la web también lee lo que un tercero escribió para manipularlo:
*"ignore previous instructions"* en un `div` invisible, la misma frase partida con un
carácter de ancho cero, o escrita con una `о` cirílica que a la vista es idéntica a la
latina. Este crate se para entre la página y el modelo, y decide qué pasa.

```rust
let triaje = Triaje::default();
match triaje.inspeccionar("https://ejemplo.test/nota", &html) {
    Veredicto::Aceptado(d)  => prompt.push_str(&envolver_como_dato(&d)),
    Veredicto::Rechazado(r) => log::warn!("rechazado en {:?}: {}", r.etapa, r.motivo),
}
```

---

## Qué demuestra

- **El texto es dato, nunca instrucción.** Lo aceptado se entrega envuelto entre
  delimitadores con una consigna explícita, y el delimitador se elige para que **no
  aparezca dentro del texto**: un contenido que intente "cerrar" el bloque desde adentro
  se encuentra con que ese no era el cierre.
- **Detección determinista, nunca un LLM.** Preguntarle a un modelo si un texto intenta
  manipularlo lo expone a esa misma manipulación. Acá se buscan frases y marcadores
  conocidos, con resultados reproducibles y explicables.
- **Normalizar antes de buscar.** NFKC, fuera caracteres invisibles y de control
  bidireccional, homoglifos plegados a su letra latina, sin tildes. Un ataque se esconde
  en la codificación antes que en las palabras.
- **Dos copias del texto, a propósito.** La que se escanea se pliega agresivamente; la
  que se entrega no. Así una página legítima en ruso o en griego sigue intacta.
- **Rechazos trazables.** Cada rechazo dice en qué etapa ocurrió y por qué, y cada
  hallazgo dice si estaba **oculto** (texto invisible en la página) u **ofuscado** (solo
  aparece después de normalizar, o sea que alguien intentó esconderlo).
- **Cero `unsafe`** (`#![forbid(unsafe_code)]`), una sola dependencia (`unicode-normalization`).

## El pipeline

```
HTML ─▶ entrada ─▶ extracción ─▶ basura ─▶ normalización ─▶ inyección ─▶ destilado
         tamaño     visible y     barato:     NFKC, invisibles,  frases y     sin duplicados,
                    oculto,       va primero  homoglifos,        marcadores   dentro del
                    por separado              tildes                          presupuesto
```

Los filtros corren en **orden de costo creciente** y el primero que falla decide. El
texto oculto (`display:none`, `hidden`, `aria-hidden`, tamaño u opacidad cero) **se
escanea igual pero nunca se entrega**.

| Módulo | Responsabilidad |
|---|---|
| `html` | Separar texto visible de oculto, descartar scripts y estilos, decodificar entidades. |
| `basura` | Rechazar páginas vacías, granjas de enlaces y texto repetido. |
| `normalizar` | Producir la copia entregable y la copia de escaneo, contando señales de ofuscación. |
| `inyeccion` | Buscar frases y marcadores de plantilla de chat, en inglés y en español. |
| `envoltorio` | Entregar lo aceptado como bloque de datos con delimitadores no colisionables. |

## Qué detecta

| Clase | Ejemplos (inglés y español) |
|---|---|
| Anulación de instrucciones | *ignore previous instructions*, *olvida tus instrucciones* |
| Suplantación de rol | *from now on you are*, *a partir de ahora eres*, *developer mode enabled* |
| Exfiltración o uso de herramientas | *reveal your system prompt*, *envía la conversación* |
| Marcador de plantilla de modelo | `<\|im_start\|>`, `[INST]`, `<<SYS>>` — aunque vengan espaciados |

Y las encuentra aunque vengan en texto oculto, partidas con caracteres de ancho cero,
con homoglifos cirílicos o griegos, en letras de ancho completo o escritas con
entidades HTML.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Un artículo legítimo pasa limpio, sin scripts ni estilos | `un_articulo_legitimo_se_acepta_limpio` |
| Palabras sensibles sueltas no bastan para rechazar | `un_articulo_que_habla_de_instrucciones_no_se_rechaza` |
| Lo oculto se escanea pero nunca se entrega | `el_texto_oculto_benigno_se_escanea_pero_no_se_entrega` |
| Inyección oculta, con invisibles, homoglifos, ancho completo o entidades | `una_inyeccion_*` (6 tests) |
| Todo rechazo tiene etapa y motivo | verificado en cada test de rechazo |
| El envoltorio no se deja cerrar desde adentro | `el_envoltorio_no_se_deja_cerrar_desde_adentro` |
| El tamaño de entrada se controla en el borde exacto | `un_documento_enorme_se_rechaza_sin_leerlo` |

Todo el contenido de prueba es ficticio.

## Correr

```bash
cargo test                                  # 37 tests
cargo run --example triar                   # cuatro páginas de muestra
cargo run --example triar -- pagina.html    # un archivo propio
```

```
── inyección escondida en un div invisible
   RECHAZADO en Inyeccion · posible inyección de prompt: anulación de instrucciones
   («ignore previous instructions», en texto oculto)
```

## Límites conocidos

- **Precisión antes que cobertura.** Las listas dejan fuera a propósito frases como
  *"run the following command"* o *"you are now"*, que rechazarían cualquier tutorial
  técnico. Un ataque redactado de otra forma puede pasar: este filtro es una capa de
  defensa, no la única. El envoltorio de datos es la segunda.
- **Falla hacia el lado seguro.** Un artículo que *cita* una inyección para explicarla
  también se rechaza.
- **No es un parser HTML5 completo.** Es un recorrido de estados suficiente para páginas
  reales. Ante HTML malformado, un elemento oculto sin cerrar deja todo lo que sigue como
  oculto: se escanea, pero no se entrega.
- **Los umbrales de basura son ilustrativos** y se ajustan en `UmbralesBasura`.

## Licencia

MIT — ver [LICENSE](LICENSE).
