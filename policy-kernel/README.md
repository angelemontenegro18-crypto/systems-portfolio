# policy-kernel

**El núcleo de política de un interbloqueo de seguridad, en `no_std`: la tabla de reglas se
valida al compilar, la autorización es una capacidad que solo la política entrega, y lo que
no está permitido no compila o no llega al hardware.**

Una prensa tiene una válvula y un pistón, y un panel desde el que se les mandan órdenes. Entre
el panel y el hardware hay un interbloqueo: una orden fuera de rango no pasa, y con el
resguardo abierto no pasa nada. La forma habitual de escribirlo son comprobaciones en tiempo
de ejecución, y su falla habitual es el camino que se saltea una. Este crate mueve las
garantías al compilador: una tabla mal hecha no compila, una orden sin evaluar no tiene tipo
para llegar al actuador, y un interbloqueo desarmado no tiene método para aplicar.

```rust
const REGLAS: [Regla; 2] = [
    Rango::<0, 80>.regla(Clase::Apertura),     // un rango vacío no compila
    Rango::<90, 100>.regla(Clase::Apertura),
];
const _: () = assert!(validar(&REGLAS));       // ordenada y sin solapamientos, al compilar

if let Decision::Permitir(autorizada) = politica.evaluar(MoverValvula { apertura: 50 }) {
    let comando = interbloqueo.aplicar(autorizada);   // solo armado, solo autorizada
}
```

---

## Qué demuestra

- **La tabla se valida al compilar.** `validar` es una `const fn` que exige la tabla ordenada
  por clase y por mínimo, sin dos reglas de la misma clase que compartan un valor, y sin
  rangos vacíos; con `const _: () = assert!(validar(&REGLAS))`, una tabla que no cumple es un
  error de compilación (doctest `compile_fail,E0080` con dos reglas solapadas). La tabla de la
  prensa se valida así.
- **Un rango vacío es un error de tipo.** `Rango<MIN, MAX>` lleva la aserción `MIN <= MAX` en
  un bloque `const`, que se evalúa al compilar (`compile_fail,E0080`).
- **La autorización es una capacidad.** `Politica::evaluar` devuelve `Permitir(Autorizada)` o
  `Denegar(motivo)`. `Autorizada` tiene campos privados (`compile_fail,E0451`): solo la
  política la fabrica, aplicarla la consume, y mientras vive tiene prestada a la política que
  la dio.
- **Órdenes selladas.** El trait `Orden` tiene un supertrait privado: un tipo de afuera no
  puede hacerse pasar por una orden (`compile_fail,E0277`). La política sabe de antemano todo
  lo que le puede llegar.
- **El estado del interbloqueo, en el tipo.** `Interbloqueo<Desarmado>` no tiene `aplicar`
  (`compile_fail,E0599`); armarlo exige el resguardo cerrado, y si no lo está, sigue
  desarmado. Armado, `aplicar` recibe una `Autorizada` y no una orden suelta
  (`compile_fail,E0308`), y devuelve el `Comando` para el hardware, que tampoco se fabrica de
  otra forma.
- **Cierra en falla.** La decisión empieza en `Denegar` y solo cambia si una regla de la clase
  contiene el valor. Una clase sin reglas se deniega siempre; una tabla vacía lo deniega todo.
  Las funciones son totales: sin `panic!`, `unwrap` ni indexación.
- **La tabla ordenada se aprovecha.** Como la validez está garantizada, el evaluador recorre
  las reglas de la clase en orden de mínimo y corta en cuanto una empieza después del valor.
- **Contra una referencia ingenua.** Un evaluador y un validador de referencia, escritos en las
  pruebas sin las optimizaciones de la biblioteca, se comparan con ella: barrido exhaustivo de
  la tabla de la prensa (las 4 clases por los 256 valores de un `u8`), 500 tablas válidas al
  azar con el mismo barrido, y 4000 tablas cualesquiera para `validar`, todo con semillas
  fijas.
- **`no_std` de verdad.** Sin `alloc` y sin dependencias; `#![forbid(unsafe_code)]` y
  `#![deny]` de `unwrap`, `expect`, `panic!` e indexación. Una prueba escanea el código fuente
  por lo mismo y comprueba que el escaneo detectaría una violación.

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Una tabla solapada no compila | doctest `compile_fail,E0080` de `regla` |
| Un rango vacío no compila | doctest `compile_fail,E0080` de `regla` |
| `validar` coincide con un validador de referencia | `validar_coincide_con_la_referencia_en_tablas_al_azar` (4000 tablas), `las_tablas_validas_por_construccion_pasan` |
| Dos rangos que comparten un solo valor se solapan; vacío o desordenado no vale | `dos_rangos_que_comparten_un_solo_valor_se_solapan`, `un_rango_vacio_invalida_la_tabla`, `una_tabla_desordenada_no_vale` |
| Los dos extremos de un rango están permitidos | `contiene_incluye_los_dos_extremos`, `los_bordes_de_la_prensa` |
| La evaluación coincide con la referencia en toda orden posible de la prensa | `barrido_exhaustivo_de_la_prensa_contra_la_referencia`, `cuantas_ordenes_permite_la_prensa` |
| …y en tablas al azar | `diferencial_contra_la_referencia_en_tablas_al_azar` (500 tablas × 1024 órdenes) |
| Cierra en falla: sin reglas, todo se deniega | `con_la_tabla_vacia_todo_se_deniega`, `los_bordes_de_la_prensa` (la purga) |
| Una autorización no se fabrica | doctest `compile_fail,E0451` de `politica` |
| `Orden` está sellado | doctest `compile_fail,E0277` de `orden` |
| Desarmado no aplica; armado no acepta una orden sin evaluar | doctests `compile_fail,E0599` y `compile_fail,E0308` de `interbloqueo` |
| Con el resguardo abierto no se arma | `con_el_resguardo_abierto_no_se_arma` |
| Solo lo autorizado llega al hardware | `una_orden_denegada_no_llega_al_hardware`, `armado_aplica_lo_autorizado_y_lo_cuenta` |
| Sin `std`, sin `alloc`, sin `unwrap`, sin indexación, sin dependencias | `tests/sin_std.rs` (4 pruebas) y los `#![deny(clippy::…)]` de `lib.rs` |

## Qué se midió

- **La tabla de la prensa** permite 279 de las 1024 órdenes posibles: 81 + 11 aperturas
  (0–80 % y 90–100 %), 161 presiones (0–160 bar), 26 avances (0–25 mm/s) y ninguna purga.
- **Diferencial:** 500 tablas válidas al azar (semillas 1 a 500), con 0 a 4 rangos por clase
  entre −20 y 280, por las 1024 órdenes: 512 000 evaluaciones, ninguna diferencia con la
  referencia.
- **Validación:** 4000 tablas cualesquiera (semillas 1 a 4000), 1316 válidas y 2684
  inválidas según la referencia: `validar` coincide en todas.

## Correr

```bash
cargo test                     # 21 pruebas y 8 doctests
cargo run --example prensa     # el panel manda órdenes; solo las autorizadas llegan
```

```
resguardo abierto: el interbloqueo sigue desarmado
resguardo cerrado: armado

MoverValvula { apertura: 30 }    → al hardware: Apertura = 30
MoverValvula { apertura: 85 }    → denegada (FueraDeRango)
MoverValvula { apertura: 95 }    → al hardware: Apertura = 95
MoverValvula { apertura: 120 }   → denegada (FueraDeRango)
FijarPresion { bar: 150 }        → al hardware: Presion = 150
FijarPresion { bar: 200 }        → denegada (FueraDeRango)
FijarAvance { mm_por_s: 25 }     → al hardware: Avance = 25
FijarAvance { mm_por_s: 26 }     → denegada (FueraDeRango)
Purgar { segundos: 5 }           → denegada (SinRegla)

aplicadas: 4; desarmado
```

## Límites conocidos

- **Una regla es un rango por clase, sin contexto.** No hay condiciones compuestas («más de
  100 bar solo con la válvula abierta») ni estado: cada orden se evalúa sola.
- **La validación en compilación es para tablas en un `const`.** `Politica::con_reglas`
  también acepta una tabla armada en ejecución, y la valida entonces: si no es válida devuelve
  `None` y no hay política, pero el error aparece al ejecutar, no al compilar.
- **El interbloqueo no vigila el resguardo.** Lo mira al armar; si se abre después, quien lo
  sabe tiene que desarmar. No hay sensores en el crate.
- **La cadena de tipos acota la API, no el hardware.** `Comando` solo sale de `aplicar`, pero un
  controlador que escriba en el hardware sin pedir un `Comando` está fuera del alcance del
  crate.
- **Una autorización no vence.** Está atada al préstamo de la política, no al tiempo: si se
  aplica mucho después de evaluarla, sigue valiendo.
- **Los valores de las órdenes son `u8`.** Las reglas aceptan rangos `i32`, pero las órdenes
  de la prensa llevan un byte, y el barrido exhaustivo es sobre ese byte.

## Licencia

MIT — ver [LICENSE](LICENSE).
