# autonomous-remediation

**Remediación autónoma que sabe cuándo no actuar: cuatro puertas que tienen que pasar todas,
la vuelta atrás armada antes de aplicar y, ante la duda, un ticket preciso para una persona.**

Reiniciar un servicio trabado, escalar réplicas o vaciar una caché a las tres de la mañana
ahorra horas de incidente, y también puede agrandarlo. Los modos de falla son conocidos: un
puntaje combinado donde una confianza alta compensa un daño alto; una vuelta atrás que está
en el plan pero no estaba armada; un dato que falta y se toma como «todo bien»; una
automatización que se da más permisos; un ticket que dice «algo falló» y nada más. Esta
política no admite ninguno de los cinco.

```rust
let envolvente = Envolvente::vacia().con(Permiso::escalar(api.clone(), 2, 6).unwrap());
let politica = Politica::nueva(envolvente, Umbrales::nuevos(0.90, 0.30).unwrap());
let mut remediador = Remediador::nuevo(politica, mi_actuador);

match remediador.atender(&propuesta) {
    Desenlace::Autoaplicada { .. } => {}                 // las cuatro puertas en verde
    Desenlace::Ticket(ticket) => println!("{ticket}"),   // qué, por qué, qué falló y cómo revertir
}
```

---

## Qué demuestra

- **Cuatro puertas en AND, no un puntaje.** Autoridad, reversibilidad, beneficio neto
  acotado y confianza. Un test recorre las dieciséis combinaciones de puertas que pasan o
  fallan: solo se autoaplica la que pasa las cuatro, y el ticket nombra exactamente las que
  fallaron. Confianza 1.0 y el mayor beneficio posible no compensan un daño colateral apenas
  por encima del tope.
- **Fallo cerrado.** Una confianza ausente, una métrica que falta, un NaN, un valor fuera de
  rango o dos datos que no coinciden (la acción parte de 3 réplicas y las métricas dicen 4)
  cierran la puerta que los necesita. Las comparaciones están escritas para que un NaN
  siempre falle.
- **Una envolvente que la remediación no puede ampliar.** La arma una persona y se entrega
  al construir la política. Después, la política solo la presta como `&`, los métodos que la
  arman la consumen por valor —un doctest `compile_fail` lo fija, verificado a mano con su
  código de error, E0507— y ninguna de las tres acciones la toca. Un test de trinquete
  atiende cientos de propuestas al azar, con éxitos, rechazos y fallas, y comprueba después
  de cada una que la política es exactamente la del principio.
- **La vuelta atrás, armada y verificada antes de aplicar.** También tiene que caer dentro
  de la envolvente: escalar de 1 a 3 réplicas no se autoaplica si volver a 1 está fuera del
  rango permitido. Si aplicar o la comprobación posterior fallan, se revierte; si revertir
  también falla, el ticket es urgente. Un actuador de prueba registra el orden de las
  llamadas, y una puerta que falla no lo toca.
- **Un ticket que alcanza para decidir.** Qué se iba a hacer, por qué, el veredicto de cada
  puerta con sus valores (`daño colateral 0.63 > tope 0.30`), qué pasó y cómo revertir.
- **Un detector pasivo por tipo.** El CUSUM de Page (*Continuous inspection schemes*,
  Biometrika, 1954) sobre tiempos entre llegadas: recibe `&[u64]` ya observados y devuelve un
  valor. Un test escanea su código fuente y falla ante cualquier forma de E/S, de lanzar
  procesos, de guardar estado o de llegar al resto del crate.
- **Resultados idénticos en cualquier plataforma.** El detector supone llegadas de Poisson y
  vigila que el ritmo baje a la mitad o se duplique; con esos dos cocientes, el único
  logaritmo es la constante `ln 2`. Sin funciones trascendentes en tiempo de ejecución, los
  resultados coinciden bit a bit con una réplica independiente escrita en Python.
- **Cero dependencias. Cero `unsafe`.**

## Garantías y cómo se prueban

| Garantía | Test |
|---|---|
| Las cuatro puertas juntas autoaplican, en orden: armar, verificar, aplicar, comprobar | `las_cuatro_puertas_juntas_autoaplican`, `la_vuelta_atras_se_arma_y_se_verifica_antes_de_aplicar` |
| Cada puerta, sola, bloquea, y no toca el actuador | `cada_puerta_sola_bloquea_el_autoaplicar`, `una_vuelta_atras_que_no_se_arma_o_no_se_verifica_bloquea` |
| Solo pasa la combinación con las cuatro en verde; el ticket nombra las que fallaron | `de_las_dieciseis_combinaciones_solo_autoaplica_la_que_pasa_las_cuatro` |
| Una puerta holgada no compensa otra que falla | `una_puerta_holgada_no_compensa_otra_que_falla` |
| Un dato ausente, inválido o inconsistente cierra su puerta | `un_dato_ausente_o_invalido_cierra_su_puerta` |
| Cada puerta que falla aparece en el ticket con sus valores, y el ticket dice cómo revertir | `cada_puerta_que_falla_aparece_nombrada_en_el_ticket_con_sus_valores` |
| Si aplicar o comprobar fallan, se revierte; si revertir falla, el ticket es urgente | `si_aplicar_falla_se_revierte`, `si_la_comprobacion_posterior_falla_se_revierte`, `si_la_vuelta_atras_tambien_falla_el_ticket_es_urgente` |
| La envolvente no se amplía con el uso | `la_envolvente_no_se_amplia_con_el_uso` (600 propuestas al azar, tres actuadores) |
| La envolvente no se puede modificar a través de la política | doctest `compile_fail` del módulo `envolvente` |
| Evaluar no cambia nada | `evaluar_es_una_funcion_pura` |
| El modelo de costos da los valores esperados y rechaza datos inválidos | `reiniciar`, `escalar`, `vaciar_la_cache`, `los_datos_invalidos_o_inconsistentes_se_rechazan` |
| El detector no tiene forma de hacer E/S | `el_detector_no_tiene_ninguna_forma_de_hacer_e_s`, `el_escaneo_detectaria_una_violacion` |
| Con el servicio sano, casi no hay falsas alarmas | `con_el_servicio_sano_casi_no_hay_falsas_alarmas` |
| Detecta que el ritmo baja o sube, y hacia dónde | `detecta_que_el_ritmo_baja`, `detecta_que_el_ritmo_sube`, `ejemplo_a_mano` |
| Un cambio sostenido persiste; una ráfaga, no | `un_cambio_sostenido_persiste_y_una_rafaga_no` |
| El detector coincide con una réplica independiente | `coincide_con_una_replica_independiente` |

## Correr

```bash
cargo test                  # 32 tests
cargo run --example demo    # del detector a la decisión; salida determinista
```

La demo analiza dos señales sintéticas y pasa seis propuestas por la política:

```
   a) reiniciar `api` — señal A: el ritmo de respuestas exitosas cayó a un tercio
      → AUTOAPLICADA
        actuador: armar la vuelta atrás → verificarla → aplicar → comprobar

   b) reiniciar `api` — señal B: una ráfaga de respuestas lentas
      → TICKET — confianza: 0.51 < mínimo 0.90
        actuador: sin tocar

   e) escalar `api` de 3 a 5 réplicas — utilización 1.4 sostenida
      → TICKET — las cuatro puertas pasaron, pero la aplicación falló: el orquestador devolvió un error de cuota; se revirtió
        actuador: armar la vuelta atrás → verificarla → aplicar → revertir

   TICKET · vaciar la caché de `catalogo`
     Por qué:  el 20 % de las lecturas devuelve entradas vencidas
     Estado:   no se aplicó
     Puertas:
       ✓ autoridad       vaciar la caché de `catalogo`: permitido
       · reversibilidad  posible (restaurar la caché de `catalogo` desde la instantánea tomada antes de vaciarla); se arma solo si las otras tres pasan
       ✗ beneficio neto  daño colateral 0.63 > tope 0.30
       ✓ confianza       0.97 ≥ mínimo 0.90
     Cómo revertir: restaurar la caché de `catalogo` desde la instantánea tomada antes de vaciarla
```

Lo medido sobre el detector, con 1000 intervalos de referencia y límite 10, en series
sintéticas de media 20 ms: 1 de 200 series sanas dio una falsa alarma en 2000 intervalos;
cuando el ritmo cae a un tercio, la alarma llega con una demora mediana de 12 intervalos
(máxima 36), y cuando se triplica, de 26 (máxima 44). La persistencia de un cambio sostenido
nunca bajó de 0.78; la de una ráfaga de 25 intervalos tuvo mediana 0.19.

## Límites conocidos

- **El modelo de costos es ilustrativo y no está calibrado.** Supone, por ejemplo, que
  reiniciar elimina todos los errores. Un sistema real mediría el beneficio y el daño; lo que
  este crate fija es qué se hace con esos números.
- **La confianza la pone quien arma la propuesta.** La demo usa la persistencia del detector;
  es una elección razonable, no la única.
- **La envolvente no limita la frecuencia.** Un ciclo de reinicios no se corta aquí: haría
  falta historia, y esta política decide cada propuesta por separado.
- **La reversibilidad se evalúa en dos tiempos.** Que exista una vuelta atrás se decide sin
  efectos; armarla es un efecto, y solo se hace si las otras tres puertas pasan. Por eso un
  ticket puede mostrarla pendiente. Una vuelta atrás armada que después no pasa la
  verificación queda a cargo del actuador.
- **El detector supone llegadas de Poisson y una referencia sana.** Con una referencia corta
  la media se estima peor y cambia la sensibilidad: con 300 intervalos de referencia, una
  ráfaga pasajera llegó a persistir hasta el final en 1 de 100 series; con 1000, la
  persistencia máxima de una ráfaga fue 0.68.
- **Las acciones son tres y cerradas.** Agregar una es cambiar el código y sus tests, no la
  configuración.

## Licencia

MIT — ver [LICENSE](LICENSE).
