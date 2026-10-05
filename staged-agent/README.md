# staged-agent

**Un agente que modifica un repositorio en tres niveles de confianza: simula sin escribir,
prepara en un worktree aislado y aplica solo el plan exacto que una persona revisó.**

Las herramientas que «arreglan» archivos suelen tener dos modos: uno que dice qué haría y otro
que lo hace. Entre los dos queda un hueco: lo que se aplica no es necesariamente lo que se
revisó. El repositorio pudo moverse, la rama preparada pudo cambiar, la herramienta pudo
correr con otra configuración. Este crate cierra ese hueco con una huella SHA-256 del plan: se
calcula al simular, se verifica al preparar y se **recalcula desde el contenido de la rama**
al aplicar.

```rust
let agente = Agente::nuevo("ruta/al/repositorio");
let plan = agente.simular("main")?;                  // nivel 1: no escribe nada
let preparado = agente.preparar(&plan)?;             // nivel 2: rama staged-agent/<huella>

// … una persona revisa `git diff main staged-agent/<huella>` …
let confirmacion = Confirmacion::escrita("aplicar c0aeae132d31");
agente.aplicar(&preparado, &plan.huella(), &confirmacion)?;   // nivel 3: solo ese plan
```

---

## Qué demuestra

- **Una simulación que de verdad no escribe.** Se comprueba de dos formas. Un test resume
  todo lo que hay bajo la raíz —`.git` incluido: rutas, tamaños, fechas de modificación y
  contenidos— antes y después de simular. Otro escanea el código fuente de los módulos que
  usa la simulación y falla si aparece una forma de escribir, de lanzar un proceso o de
  llamar a los módulos que escriben; en el único que habla con `git`, además, cada cadena
  literal tiene que ser un argumento de solo lectura (`rev-parse`, `ls-tree`, `cat-file`,
  `diff-tree`, `merge-base`).
- **Preparar sin tocar nada de la persona.** El plan se aplica en un worktree de `git` en un
  directorio temporal propio, sobre una rama nueva. El árbol de trabajo, el índice, `HEAD` y
  la rama destino quedan byte a byte como estaban. Una rama que ya existía con el mismo
  nombre no se toca.
- **Sin restos si algo falla.** Una guarda con `Drop` quita el worktree, su directorio y la
  rama creada, falle lo que falle a mitad de camino. Solo borra lo que creó esa preparación:
  el directorio temporal se crea con `create_dir`, que falla si ya existe.
- **El commit se verifica contra el plan.** Un gancho o un filtro de `git` (por ejemplo, la
  normalización de finales de línea de `.gitattributes`) pueden cambiar lo que se commitea.
  La huella se recalcula desde el commit preparado; si no coincide, se deshace.
- **Aplicar solo el plan exacto.** Antes de tocar nada: la confirmación tiene que ser el texto
  `aplicar <huella corta>` de ese plan (un «sí» no alcanza), la rama en uso tiene que ser la
  destino, el árbol tiene que estar limpio (archivos sin seguimiento incluidos), la rama
  preparada tiene que ser un solo commit sobre la punta actual, y la huella **recalculada
  desde su contenido** tiene que ser la revisada. No se confía en lo que diga `Preparado`.
  Recién entonces, `merge --ff-only`.
- **Rutas que no salen de la raíz.** Nada de `..`, rutas absolutas, `\` ni `:` (unidades y
  flujos alternativos de Windows). Ningún componente puede ser un enlace simbólico, aunque
  apunte adentro, y la forma canónica tiene que ser la ruta recorrida.
- **`git` sin shell.** El `git` del sistema con `std::process::Command`: siempre `-C <raíz>`
  y `-c core.autocrlf=false`, entrada estándar cerrada, sin preguntas por terminal, `--`
  antes de cada ruta, `--literal-pathspecs` al agregar, y nombres de rama validados antes de
  llegar a la línea de comandos.
- **Reglas componibles e idempotentes.** El trabajo concreto lo hace un trait `Regla`; las
  tres incluidas normalizan finales de línea, espacios finales y el salto final. Volver a
  correr el agente sobre su propio resultado no propone nada.
- **Pruebas herméticas.** Cada test arma su repositorio bajo `std::env::temp_dir()` y lo
  borra: configuración global y del sistema apuntando a un archivo vacío propio, ganchos a un
  directorio vacío, identidad local, sin firma y sin red. Si no hay `git`, fallan con un
  mensaje claro: no se omiten en silencio.
- **Cero `unsafe`.**

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Simular no escribe nada, ni en el árbol ni en `.git` | `simular_no_escribe_nada` |
| El código de la simulación no contiene ninguna forma de escribir | `los_modulos_de_la_simulacion_no_contienen_ninguna_forma_de_escribir`, `el_escaneo_detectaria_una_violacion` |
| El plan propone exactamente lo que cambia, sobre lo commiteado | `el_plan_propone_exactamente_lo_que_cambia`, `simular_lee_lo_commiteado_no_el_arbol_de_trabajo` |
| La huella es determinista y depende de la base, las reglas y los contenidos | `la_huella_es_determinista_y_depende_de_todo_el_plan` |
| Los enlaces simbólicos del repositorio no se tocan | `los_enlaces_simbolicos_del_repositorio_se_omiten` |
| Preparar no toca el árbol, el índice, `HEAD` ni la rama destino | `preparar_no_toca_el_arbol_principal_ni_la_rama_destino` |
| La rama preparada es un commit con exactamente el plan | `la_rama_preparada_es_un_commit_con_exactamente_el_plan`, `preparar_no_deja_worktrees_registrados` |
| Si preparar falla a medias, no queda worktree ni rama | `si_preparar_falla_a_medias_no_queda_worktree_ni_rama` |
| Un commit que no reproduce el plan se deshace | `si_el_commit_no_reproduce_el_plan_se_deshace` |
| Si el worktree no tiene los bytes revisados, no se escribe | `si_el_worktree_no_tiene_lo_que_el_plan_espera_no_se_escribe` |
| Una rama ajena con el mismo nombre no se toca | `una_rama_preexistente_con_ese_nombre_no_se_toca` |
| Un plan viejo o vacío no se prepara | `un_plan_viejo_no_se_prepara`, `un_plan_vacio_no_se_prepara` |
| Aplicar lleva el plan exacto a la rama destino | `aplicar_lleva_el_plan_exacto_a_la_rama_destino` |
| Una rama alterada después de prepararla se rechaza | `aplicar_rechaza_un_plan_alterado_despues_de_prepararlo`, `aplicar_rechaza_commits_agregados_a_la_rama_preparada`, `aplicar_rechaza_una_huella_que_no_es_la_preparada` |
| Sin confirmación explícita de ese plan, no se aplica | `aplicar_rechaza_sin_confirmacion_explicita_de_este_plan` |
| Con el árbol sucio, no se aplica | `aplicar_rechaza_un_arbol_sucio` |
| Si la destino se movió o la rama en uso es otra, no se aplica | `aplicar_rechaza_si_la_rama_destino_se_movio`, `aplicar_rechaza_si_la_rama_en_uso_es_otra` |
| Los nombres de rama peligrosos no llegan a `git` | `las_ramas_inexistentes_o_peligrosas_se_rechazan`, `aplicar_rechaza_nombres_de_rama_peligrosos` |
| Otra ejecución puede aplicar con solo la rama y la huella | `aplicar_desde_otra_ejecucion_con_solo_la_rama_y_la_huella` |
| Las rutas no salen de la raíz | `se_rechazan_las_rutas_que_podrian_salir_de_la_raiz`, `resolver_dentro_encuentra_lo_que_existe_y_nada_mas` |
| Los enlaces simbólicos en disco se rechazan, aunque apunten adentro (solo Unix) | `un_directorio_enlazado_hacia_afuera_se_rechaza`, `un_archivo_enlazado_se_rechaza_aunque_apunte_adentro`, `la_raiz_puede_ser_un_enlace` |
| Las reglas y su composición son idempotentes | `las_reglas_y_su_composicion_son_idempotentes` (2 000 textos al azar) |

## Correr

```bash
cargo test                  # 40 tests; 37 en Windows
cargo run --example demo    # los tres niveles sobre un repositorio temporal; salida determinista
```

Las pruebas necesitan `git` en el `PATH`. Tres de ellas crean enlaces simbólicos y solo
corren en Unix: en Windows crearlos exige permisos especiales, así que allí se omiten y
quedan 37.

```
1 · Simular: calcular el plan sin escribir nada

   plan c0aeae132d31 sobre main (ae7d41540fca)
     M docs/espacios.md  (espacios finales)
     M sin_final.txt  (salto final)
     M windows.txt  (finales de línea)
     - logo.bin  (omitido: Binario)

   ¿Cambió el árbol de trabajo? no

3 · Aplicar: solo el plan exacto, con confirmación

   con «sí»:
     rechazado — falta la confirmación explícita de este plan
   con «aplicar c0aeae132d31», pero con el árbol sucio:
     rechazado — el árbol de trabajo tiene cambios: README.md
   con «aplicar c0aeae132d31» y el árbol limpio:
     main avanza a 01a56724678d (3 archivos)
```

## Límites conocidos

- **Solo modifica archivos que ya existen.** No crea, no borra, no renombra y no cambia
  permisos; un commit preparado que haga cualquiera de esas cosas no pasa la verificación. Los
  binarios, los enlaces simbólicos y los submódulos se dejan de lado y el plan lo dice.
- **Trabaja sobre lo commiteado.** Lo que haya sin commitear no entra en el plan, y aplicar
  exige un árbol limpio.
- **La confirmación no es autenticación.** Evita aplicar por accidente, o aplicar otro plan;
  no protege de alguien con acceso al repositorio.
- **Los ganchos del repositorio corren.** `git commit` en el worktree y `git merge` al aplicar
  ejecutan los ganchos configurados. Si cambian el contenido, la verificación lo detecta; lo
  demás que hagan es política del repositorio.
- **Un proceso de `git` por archivo al simular.** Pensado para repositorios chicos y medianos.
- **Una ejecución a la vez por repositorio.** Si la rama destino se mueve entre simular y
  aplicar, se detecta y hay que volver a simular; dos agentes simultáneos no se coordinan.
- **Una barrera que en Linux no se puede probar.** La comparación de la forma canónica cubre
  los sistemas de archivos que no distinguen mayúsculas, donde dos rutas del árbol pueden ser
  el mismo archivo. En Linux nada llega a ella sin pasar antes por un enlace simbólico, que
  ya se rechaza.
- **La huella corta tiene 48 bits.** Es la que va en el nombre de la rama y en la
  confirmación; la que se compara al aplicar es la completa.

## Licencia

MIT — ver [LICENSE](LICENSE).
