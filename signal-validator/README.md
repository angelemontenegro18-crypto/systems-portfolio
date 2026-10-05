# signal-validator

**Validar una señal en una serie temporal sin engañarse: validación cruzada con purga y
embargo, permutación por bloques, control de falsos descubrimientos y una reserva que se
abre una sola vez.**

En una serie temporal es fácil encontrar una señal, y casi siempre es mentira. Cuatro errores
la fabrican: validar con pliegues barajados, que dejan observaciones casi idénticas a ambos
lados; medir la significancia permutando de a un valor, que ignora la dependencia; probar
muchas hipótesis y quedarse con las que pasan; y mirar la muestra de confirmación más de una
vez. Este crate cubre los cuatro, sin dependencias.

```rust
let pliegues = particionar(&intervalos, 5, embargo)?;        // purga y embargo
let prueba = prueba_por_bloques(&x, &y, 50, 999, &mut g, |a, b| correlacion(a, b).abs())?;
let elegidas = benjamini_hochberg(&p_valores, 0.10)?;        // FDR controlada

let reserva = MuestraSellada::sellar(datos_finales);         // compromiso SHA-256
let datos = reserva.abrir();                                 // una vez: después no existe
```

---

## Qué demuestra

- **Purga y embargo.** Cada observación declara el tramo de tiempo del que depende, desde el
  primer dato que usan sus características hasta el último que usa su etiqueta. Para cada
  bloque de validación se saca del entrenamiento todo lo que se cruza con ese tramo, y
  además lo que empieza poco después: el embargo cubre la memoria que los intervalos no
  declaran. La idea de dejar un margen alrededor del bloque es la de la validación cruzada
  para datos dependientes, *h-block* (Burman, Chow y Nolan, 1994, *Biometrika*) y *hv-block*
  (Racine, 2000, *Journal of Econometrics*).
- **Permutación por bloques, con un p-valor que nunca es cero.** Se reordenan bloques
  contiguos, no valores sueltos, para conservar la dependencia de la serie (la idea del
  remuestreo por bloques: Künsch, 1989, *Annals of Statistics*; Politis y Romano, 1994,
  *Journal of the American Statistical Association*). El p-valor es `(1 + r)/(1 + B)`: el
  dato observado cuenta como una permutación más (Phipson y Smyth, 2010, *Statistical
  Applications in Genetics and Molecular Biology*). Un estadístico permutado NaN cuenta en
  contra: el p-valor sube, nunca baja.
- **Benjamini–Hochberg** (1995, *Journal of the Royal Statistical Society, Series B*), con
  p-valores ajustados, verificado contra `p.adjust` de R.
- **Una reserva que el compilador no deja abrir dos veces.** `abrir(self)` consume la
  muestra; no implementa `Clone`; sus datos no se leen sin abrirla y su `Debug` no los
  muestra. Tres doctests `compile_fail` lo fijan, cada uno con su código de error
  (E0382, E0599 y E0616), comprobados a mano para que fallen por la razón correcta.
- **Un compromiso que se puede anotar antes del análisis.** Al sellar se calcula el SHA-256
  de una codificación canónica del contenido: después, cualquiera puede comprobar que la
  muestra que se abrió es la que se reservó. SHA-256 escrito a mano y verificado contra la
  norma.
- **Una marca para lo que el compilador no ve.** Otra ejecución del programa puede volver a
  sellar y abrir; `abrir_con_marca` deja un archivo testigo con `create_new` y se niega si ya
  existe, aun con dos ejecuciones a la vez. **Es disciplina, no seguridad.**
- **Datos idénticos bit a bit en cualquier plataforma.** El generador es SplitMix64 (Steele,
  Lea y Flood, 2014, OOPSLA) y la normal es la suma de 12 uniformes menos 6: solo sumas.
  Box–Muller depende de `ln`, `cos` y `sin` de la biblioteca del sistema, que pueden
  diferir en el último bit.
- **Cada pieza, contra una referencia independiente.** El generador, contra
  `java.util.SplittableRandom`; BH y la normal, contra R; SHA-256 y el compromiso, contra
  Python; la demo de fuga, contra una réplica en Python que da los mismos aciertos en todos
  sus dígitos.
- **Cero dependencias. Cero `unsafe`.**

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Ninguna observación de entrenamiento comparte un instante con su validación | `ninguna_observacion_de_entrenamiento_comparte_informacion_con_su_validacion` (300 configuraciones al azar) |
| El embargo aparta lo que empieza justo después del bloque | `el_embargo_aparta_lo_que_sigue_a_la_validacion` |
| Cada observación se valida exactamente una vez, y no se aparta nada sin motivo | `cada_observacion_se_valida_exactamente_una_vez`, `solo_se_aparta_lo_que_hace_falta` |
| Sobre ruido, el k-fold barajado inventa señal y la partición purgada no | `sobre_ruido_el_kfold_barajado_inventa_senal_y_el_purgado_no` |
| El p-valor nunca es cero | `el_p_valor_nunca_es_cero`, `el_p_valor_cuenta_al_observado_como_una_permutacion_mas` |
| Un estadístico permutado NaN sube el p-valor | `un_estadistico_permutado_nan_cuenta_como_extremo` |
| Con dependencia, permutar de a uno da falsos positivos; por bloques, no | `con_dependencia_permutar_de_a_uno_da_falsos_positivos_y_por_bloques_no` |
| Una señal débil (correlación ≈ 0,15) se detecta | `detecta_una_senal_debil_plantada` |
| BH es de subida y coincide con R | `ejemplo_a_mano`, `es_de_subida`, `coincide_con_r`, `rechazar_por_ajustado_equivale_al_procedimiento` |
| BH controla la FDR; un umbral fijo no | `controla_la_fdr_en_simulacion`, `cien_nulas_dan_cinco_falsos_descubrimientos_sin_corregir` |
| La reserva no se abre dos veces, no se copia y no se espía | doctests `compile_fail` del módulo `sellado` |
| La marca impide otra apertura, también desde dos ejecuciones a la vez | `la_marca_impide_una_segunda_apertura`, `dos_ejecuciones_a_la_vez_no_pueden_abrir_las_dos` |
| Un error al dejar la marca no abre ni destruye la muestra | `si_no_se_puede_dejar_la_marca_no_se_abre` |
| El compromiso coincide con una implementación externa y cambia con cualquier bit | `el_compromiso_coincide_con_una_implementacion_externa`, `cualquier_cambio_en_los_datos_cambia_el_compromiso` |
| Separar diseño y reserva purga el diseño, no la reserva | `separar_purga_del_diseno_lo_que_toca_la_reserva` |
| El `Debug` no muestra los datos | `el_debug_no_muestra_los_datos` |
| El generador coincide bit a bit con la referencia y no tiene sesgo | `coincide_con_la_implementacion_de_referencia`, `uniforme_y_normal_son_reproducibles_bit_a_bit`, `entero_bajo_no_tiene_sesgo_de_modulo`, `barajar_da_todas_las_permutaciones_por_igual` |
| SHA-256 según la norma, también en los bordes del relleno | `vectores_de_la_norma`, `largos_en_los_bordes_del_relleno` |

## Correr

```bash
cargo test                              # 52 tests
cargo run --release --example demo      # tres escenarios, alrededor de un segundo
cargo run --release --example barrido   # los mismos escenarios en muchas semillas
```

La demo arma tres escenarios con semillas fijas. Un extracto:

```
1 · Ruido puro: ¿cuánto «acierta» un modelo que no puede acertar?

   semilla   k-fold barajado   bloques con purga y embargo
         1              67.7 %                         49.5 %
         …
     media              67.4 %                         50.6 %

2 · Una señal débil entre veinte candidatas

   Diseño (2000 observaciones), permutación por bloques de 50, 999 permutaciones:
     candidata #7   p = 0.001  ← elegida por BH (q = 0.10)
     candidata #8   p = 0.078
     …
   La reserva se abre una sola vez, solo para las elegidas:
     candidata #7   p = 0.001  → confirmada

3 · Cien hipótesis a la vez, 1000 repeticiones

   90 nulas y 10 con efecto real (media 0.8):
     umbral fijo 0.05: 30.7 % de lo declarado es falso; encuentra el 94.5 % de los efectos
     BH, q = 0.05:     4.6 % de lo declarado es falso; encuentra el 75.3 % de los efectos
```

El barrido repite los escenarios para que ninguna cifra dependa de una semilla con suerte.
En 30 semillas, el k-fold barajado «acertó» sobre ruido entre 63,6 % y 71,8 %; la partición
purgada, entre 43,8 % y 59,1 %, alrededor del 50 % verdadero. En 40 semillas, la señal débil
fue elegida en el diseño 39 veces y confirmada en la reserva 33. De las 12 candidatas sin
señal que el diseño dejó pasar —BH controla la proporción de falsos descubrimientos, no los
elimina—, 3 pasaron también la reserva. Es más de lo esperable (0,6), pero cada una es una prueba
nueva al 5 %, y el mismo barrido mide esa prueba sola sobre 1200 casos sin señal: rechaza el
5,2 %.

## Límites conocidos

- **La permutación por bloques es aproximada.** En las uniones entre bloques se corta la
  dependencia, y cuando la memoria de la serie es larga comparada con el bloque, la prueba
  rechaza algo más de lo prometido: con dos series AR(1) de φ = 0,8 y bloques de 25, el
  barrido mide 7,0 % en vez de 5 % (permutando de a uno, 37,3 %). Bloques más largos lo
  achican, a costa de menos permutaciones distintas.
- **El p-valor de permutación tiene resolución 1/(B + 1).** Con `m` hipótesis y BH al nivel
  `q`, hacen falta al menos `m/q − 1` permutaciones para poder rechazar alguna: con 20
  candidatas y `q = 0,10`, 199.
- **BH garantiza la FDR con pruebas independientes o con dependencia positiva** (Benjamini y
  Yekutieli, 2001, *Annals of Statistics*). Con dependencia arbitraria, no.
- **La purga es tan buena como los intervalos.** Si una característica usa datos fuera de lo
  que declara, la purga no lo ve; el embargo cubre parte de eso, no todo.
- **La marca es disciplina, no seguridad.** Quien quiera mirar dos veces puede borrarla o
  volver a sellar los datos de origen. Lo que se evita es hacerlo sin darse cuenta.
- **El compromiso compromete, no oculta.** Quien tenga los datos puede recalcularlo, que es
  lo que se busca; no sirve para esconder la reserva.
- **La cola de la normal es una aproximación** (fórmula 7.1.26 de Abramowitz y Stegun,
  *Handbook of Mathematical Functions*, 1964), con error absoluto menor que 1,5·10⁻⁷:
  alcanza para umbrales del orden de 10⁻⁴, no para p-valores minúsculos.
- **Los datos sintéticos usan una normal aproximada**, con colas que terminan en ±6. Sirve
  para demostrar; no para estudiar colas.

## Licencia

MIT — ver [LICENSE](LICENSE).
