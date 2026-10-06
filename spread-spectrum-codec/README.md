# spread-spectrum-codec

**Un módem de espectro ensanchado por secuencia directa, `no_std` y con aritmética entera:
cada bit viaja como N chips, y un tono más fuerte que la señal no alcanza a voltearlo.**

Los sensores de una planta —niveles de tanques, presiones— comparten un enlace con ruido, y
el variador de una bomba mete un tono justo en la banda. La respuesta clásica es la de GPS,
CDMA y 802.15.4: transmitir cada bit multiplicado por una secuencia conocida de `N` chips, y
en el receptor correlar con esa misma secuencia. La señal se suma en fase; el tono y el ruido
se reparten. Este crate arma la cadena completa —secuencias, códigos, corrección de errores,
entrelazado y sincronía— sin `alloc`, sin dependencias y sin pánicos, y **mide** cuánto gana
en vez de prometerlo.

```rust
let palabra = codificar(0b1011)?;                       // Hamming (8,4)
let codigo = Codigo::secuencia_m(Polinomio::GRADO_5);   // 31 chips por bit
esparcir(&bits, &codigo, 100, &mut muestras)?;
// … el canal suma un desplazamiento de 140 por muestra, más que la amplitud de chip …
decidir(&muestras, &codigo, &mut recibidos)?;           // la correlación lo reparte
assert_eq!(decodificar(palabra), Decodificacion::Intacta(0b1011));
```

---

## Qué demuestra

- **Secuencias de máximo largo, verificadas grado por grado.** Un registro de desplazamiento
  con un polinomio primitivo de grado `n` recorre los `2ⁿ − 1` estados no nulos, y su salida
  tiene tres propiedades (Golomb, *Shift Register Sequences*, 1967): periodo `2ⁿ − 1`, un 1
  más que ceros, y autocorrelación de dos valores —`N` sin desfase, `−1` con cualquier otro.
  Las tres se comprueban por prueba **en cada grado de 3 a 7**, y la máscara de realimentación
  se coteja con la recurrencia del polinomio escrita a mano en la notación de las tablas.
- **Códigos Gold con su cota, comprobada entera.** De un par preferente sale una familia de
  `N + 2` códigos cuya correlación, para cualquier par y cualquier desfase, toma solo tres
  valores: `−1`, `−t` y `t − 2` (Gold, *IEEE Transactions on Information Theory*, 1967;
  Sarwate y Pursley, *Proceedings of the IEEE*, 1980). Se verifica de forma exhaustiva en las
  familias de 31, 63 y 127 chips —todos los pares, todos los desfases— y se comprueba que los
  tres valores aparecen. Un control negativo muestra que dos secuencias m que no forman par
  preferente rompen la cota.
- **Esparcir y desesparcir con enteros.** Muestras `i32`, sumas `i64`. El receptor entrega la
  decisión dura (el signo) y la blanda (la correlación entera), y recorta los valores blandos
  a `N · amplitud` para que una ráfaga enorme no pese más que un bit seguro.
- **Hamming ampliado (8,4), exhaustivo.** Corrige cualquier error de un bit y detecta
  cualquiera de dos (Hamming, *Bell System Technical Journal*, 1950): los 16 datos con los 8
  errores simples y los 28 dobles, uno por uno. La decodificación blanda elige, de las 16
  palabras válidas, la de mayor correlación.
- **Un entrelazador con la cota justa.** Una ráfaga de hasta `D` bits toca cada palabra a lo
  sumo una vez —comprobado para `D` de 1 a 16 en todas las posiciones— y una de `D + 1` ya no.
  Junto con Hamming, corrige cualquier ráfaga de hasta `D` bits.
- **Sincronía por correlación contra un preámbulo**, con umbral de media altura. Manda la
  primera posición que alcanza el umbral, no el máximo del búfer: una ráfaga fuerte después
  del preámbulo correla más que él y no debe desplazarlo.
- **La ganancia de procesamiento, medida.** Para `N` de 7 a 127, el tono más fuerte que se
  tolera con menos de un error cada mil bits, comparado con `10 · log₁₀(N)`. La tabla está
  más abajo, con sus semillas.
- **`no_std` de verdad.** `#![no_std]`, sin `alloc`, sin dependencias, `#![forbid(unsafe_code)]`
  y `#![deny]` de `unwrap`, `expect`, `panic!` e indexación sin verificar. Una prueba escanea
  el código fuente por lo mismo, y comprueba que el escaneo detectaría una violación.
- **Pruebas deterministas.** El canal simulado —ruido gaussiano como suma de 12 uniformes
  enteras, un tono, ráfagas— usa un generador propio con semillas fijas, y vive en `tests/`:
  no es parte de la biblioteca.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Para cada grado de 3 a 7, periodo `2ⁿ − 1` sin pasar dos veces por un estado | `el_periodo_es_dos_a_la_n_menos_uno_en_cada_grado` |
| Para cada grado, un 1 más que ceros | `en_un_periodo_hay_un_uno_mas_que_ceros` |
| Para cada grado, autocorrelación `N` sin desfase y `−1` con cualquier otro | `la_autocorrelacion_vale_n_sin_desfase_y_menos_uno_con_cualquier_otro` |
| La realimentación es la del polinomio de la tabla | `la_salida_cumple_la_recurrencia_del_polinomio` (referencia escrita a mano) |
| Gold: toda correlación de la familia toma solo los tres valores, y los tres aparecen | `familia_de_31_tres_valores_exhaustivo`, `familia_de_63_…`, `familia_de_127_…` |
| La cota es del par preferente, no de dos secuencias cualesquiera | `una_secuencia_m_y_su_inversa_no_forman_par_preferente` |
| Ida y vuelta sin ruido, con cada código y por toda la cadena | `ida_y_vuelta_sin_ruido_con_cada_codigo`, `ida_y_vuelta_sin_ruido_por_toda_la_cadena` |
| Un desplazamiento constante se tolera hasta exactamente `N · amplitud` | `un_desplazamiento_constante_se_tolera_hasta_n_por_la_amplitud` |
| Tres sensores con códigos Gold comparten el canal sin un error | `tres_sensores_comparten_el_canal_con_codigos_gold` |
| Hamming (8,4): todo error de un bit se corrige y todo error de dos se detecta | `cualquier_error_de_un_bit_se_corrige`, `cualquier_error_de_dos_bits_se_detecta` |
| Una ráfaga de hasta `D` bits toca cada palabra a lo sumo una vez, y la cota es justa | `una_rafaga_de_hasta_d_bits_toca_cada_palabra_a_lo_sumo_una_vez`, `una_rafaga_de_d_mas_uno_bits_ya_toca_una_palabra_dos_veces` |
| Con Hamming, cualquier ráfaga de hasta `D` bits se corrige | `con_hamming_cualquier_rafaga_de_hasta_d_bits_se_corrige` (`D` de 1 a 12, toda posición) |
| El pico cae exactamente donde empieza el preámbulo, con ruido | `el_pico_cae_donde_empieza_el_preambulo` |
| Sin preámbulo no hay trama; el umbral separa una trama al 40 % de una al 60 % | `sin_preambulo_ni_el_ruido_ni_los_datos_llegan_a_media_altura`, `el_umbral_separa_una_trama_al_cuarenta_por_ciento_de_una_al_sesenta` |
| Una ráfaga fuerte después del preámbulo no lo desplaza | `una_rafaga_fuerte_despues_del_preambulo_no_lo_desplaza` |
| Con ruido, un tono del doble de la señal y una ráfaga de `D` bits, llegan las 16 lecturas | `una_rafaga_de_d_bits_con_ruido_y_tono_se_corrige_entera` |
| Con ruido gaussiano, la decisión blanda recupera más que la dura | `con_ruido_gaussiano_la_decision_blanda_recupera_mas_lecturas_que_la_dura` |
| El recorte salva a la decisión blanda de una ráfaga enorme | `recortar_los_blandos_salva_a_la_decision_blanda_de_una_rafaga_enorme` |
| La ganancia medida crece con `N`; sin ruido, el tono tolerado es el de la tabla y queda a menos de 1,5 dB de `10 · log₁₀(N)` | `con_ruido_la_ganancia_medida_crece_con_n`, `sin_ruido_el_tono_tolerado_es_el_de_la_tabla_y_queda_a_menos_de_1_5_db_de_10_log_n` |
| Ni polinomios ni pares preferentes inventados: no se construyen desde fuera | doctests `compile_fail,E0451` de `lfsr` y `gold` |
| Sin `std`, sin `alloc`, sin `unwrap`, sin indexación sin verificar, sin dependencias | `tests/sin_std.rs` (4 pruebas) y los `#![deny(clippy::…)]` de `lib.rs` |
| Las entradas extremas saturan en vez de desbordar | `la_amplitud_extrema_satura_en_vez_de_desbordar`, `la_decision_blanda_no_desborda_con_valores_extremos` |

## Qué se midió

**Ganancia de procesamiento.** Amplitud de chip 1000, ruido de σ = 100 por chip, un tono de
periodo 16 chips, 200 000 bits por intento; semillas 2026 para los bits y 2027 para el ruido.
Para cada `N`, la amplitud entera de tono más alta con menos de un error cada mil bits
(búsqueda binaria, con las mismas semillas en cada intento), comparada con la de un bit sin
esparcir. La columna «sin ruido» repite la medición con el ruido apagado.

| N | secuencia | tono tolerado | ganancia medida | sin ruido: tono | sin ruido: ganancia | 10·log₁₀(N) |
|---:|---|---:|---:|---:|---:|---:|
| 1 | (sin esparcir) | 772 | — | 999 | — | — |
| 7 | grado 3 | 2453 | 10.04 dB | 2678 | 8.56 dB | 8.45 dB |
| 15 | grado 4 | 3816 | 13.88 dB | 4065 | 12.19 dB | 11.76 dB |
| 31 | grado 5 | 6195 | 18.09 dB | 6458 | 16.21 dB | 14.91 dB |
| 63 | grado 6 | 7788 | 20.08 dB | 8002 | 18.07 dB | 17.99 dB |
| 127 | grado 7 | 10015 | 22.26 dB | 10207 | 20.19 dB | 21.04 dB |

Sin ruido, la ganancia queda a menos de 1,3 dB de `10 · log₁₀(N)` en los cinco largos; lo que
la aparta es cuánto responde cada secuencia a la frecuencia del tono, que no es la misma para
todas. Con ruido sale entre 1,5 y 2,1 dB más alta: la amplitud de chip es fija, así que un bit
esparcido dura `N` chips y junta más energía, y el que más margen pierde frente al ruido es el
bit sin esparcir (de 999 a 772). Sin ruido el resultado no depende de cuántos bits se miren
—lo decide la peor combinación de fase del tono y signo del bit—, y una prueba lo fija con
2 000 bits.

**Decisión dura contra blanda.** 400 tramas de 8 lecturas, `N = 31`, ruido de σ = 3,4 veces
la amplitud de chip (cerca de un 5 % de bits volteados): la decisión dura recupera 3009 de
3200 lecturas; la blanda, 3183. Las cifras las imprime
`cargo test --test enlace -- --nocapture`.

## Correr

```bash
cargo test                                # 59 pruebas y 3 doctests
cargo run --example enlace                # una trama por un canal con ruido, tono y ráfaga
cargo run --release --example ganancia    # la tabla de arriba, unos 20 s
```

El ejemplo del enlace manda ocho lecturas con un código Gold de 31 chips, por un canal con
ruido de σ igual a la amplitud de chip, un tono del doble de esa amplitud y una ráfaga de
ruido 30 veces más fuerte sobre seis bits:

```
trama: 127 chips de preámbulo + 64 bits × 31 chips, desde la muestra 500
canal: ruido σ = 1000 y tono de amplitud 2000 por chip (la señal: 1000), ráfaga sobre los bits 20 a 25
preámbulo en la muestra 500; 3 de 64 bits llegaron volteados

| lectura | enviada | decisión dura | decisión blanda |
|---:|---:|---|---:|
| 0 | 7 | 7 (intacta) | 7 |
| 1 | 8 | 8 (intacta) | 8 |
| 2 | 8 | 8 (intacta) | 8 |
| 3 | 9 | 9 (intacta) | 9 |
| 4 | 11 | 11 (corregida) | 11 |
| 5 | 12 | 12 (intacta) | 12 |
| 6 | 12 | 12 (corregida) | 12 |
| 7 | 10 | 10 (corregida) | 10 |
```

## Límites conocidos

- **Es un modelo a nivel de chip, en banda base.** Modulación de fase binaria sin portadora
  ni filtros, y el receptor asume que la muestra `k` es el chip `k`: no recupera el reloj de
  chip ni corrige corrimientos de frecuencia.
- **La ganancia se midió contra un solo tono**, de periodo 16 chips. Cada secuencia responde
  distinto a cada frecuencia; con otro tono, los números serían otros.
- **El ruido es una aproximación:** la suma de 12 uniformes, sin colas más allá de seis
  desviaciones. Sirve para medir tasas de error del orden de 10⁻³, no para colas mucho más
  finas.
- **Tres errores en una palabra se decodifican mal:** se parecen a uno y salen «corregidos»
  hacia otro dato (`con_tres_errores_el_dato_sale_mal`). Una ráfaga de más de `D` bits ya
  puede poner dos errores en una palabra.
- **Una ráfaga fuerte antes del preámbulo engaña a la sincronía**
  (`una_rafaga_fuerte_antes_del_preambulo_engana_a_la_busqueda`). La trama no lleva un código
  de detección propio que delate un enganche falso; Hamming detecta solo parte.
- **La decisión blanda necesita el recorte.** Sin recortar los valores blandos a
  `N · amplitud`, una ráfaga enorme arrastra a su palabra
  (`recortar_los_blandos_salva_a_la_decision_blanda_de_una_rafaga_enorme`); el receptor de las
  pruebas recorta.
- **No cifra.** Los códigos son públicos y cualquiera que los conozca puede desesparcir: el
  esparcido protege contra el ruido y la interferencia, no da confidencialidad.
- **Tabla cerrada:** un polinomio por grado de 3 a 7 y tres pares preferentes (31, 63 y 127).
  Un [`Codigo`](src/lfsr.rs) ocupa siempre 127 chips de memoria, aunque sea más corto.
- **El simulador del canal no es API.** Vive en `tests/canal/`, y los ejemplos lo incluyen con
  `#[path]`.

## Licencia

MIT — ver [LICENSE](LICENSE).
