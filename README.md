# systems-portfolio

Nueve proyectos independientes en Rust, cada uno con su propio README, sus pruebas y sus
ejemplos. No comparten código ni dependen entre sí.

| Proyecto | Qué demuestra | In English |
| :--- | :--- | :--- |
| [`session-isolation`](session-isolation/) | Aislamiento de sesiones por proceso, en Rust: una instantánea compartida lock-free y sin `unsafe`, un proceso efímero por petición, y un pool que reemplaza procesos en caliente sin perder el slot. | Per-process session isolation in Rust: a lock-free shared snapshot without `unsafe`, one ephemeral process per request, and a pool that hot-swaps processes without losing the slot. |
| [`content-triage`](content-triage/) | Triaje determinista de contenido web no confiable, antes de que llegue a un modelo de lenguaje. | Deterministic triage of untrusted web content before it reaches a language model. |
| [`modbus-ro`](modbus-ro/) | Cliente Modbus TCP de solo lectura, en Rust. La escritura no está prohibida: no existe. | A read-only Modbus TCP client in Rust. Writing isn't forbidden: it doesn't exist. |
| [`sealed-checkpoint`](sealed-checkpoint/) | Checkpoints de estado sellados con XChaCha20-Poly1305: escritura atómica que sobrevive a un corte de luz, encabezado autenticado, sellos ligados a su nombre y protección contra retroceso. | State checkpoints sealed with XChaCha20-Poly1305: atomic writes that survive a power cut, an authenticated header, seals bound to their name, and rollback protection. |
| [`polite-crawler`](polite-crawler/) | Un crawler honesto, en Rust: dice quién es, pide permiso y no apura a nadie. | An honest crawler in Rust: it says who it is, asks for permission, and rushes no one. |
| [`tool-gateway`](tool-gateway/) | Registro de herramientas para modelos de lenguaje con *function calling*: solo funciones puras, una lista de exclusión verificada por test, y argumentos validados antes de llamar. | A tool registry for language models with function calling: pure functions only, an exclusion list verified by tests, and arguments validated before each call. |
| [`signal-validator`](signal-validator/) | Validar una señal en una serie temporal sin engañarse: validación cruzada con purga y embargo, permutación por bloques, control de falsos descubrimientos y una reserva que se abre una sola vez. | Validating a signal in a time series without fooling yourself: purged and embargoed cross-validation, block permutation, false-discovery control, and a holdout that can only be opened once. |
| [`staged-agent`](staged-agent/) | Un agente que modifica un repositorio en tres niveles de confianza: simula sin escribir, prepara en un worktree aislado y aplica solo el plan exacto que una persona revisó. | An agent that changes a repository at three levels of trust: it simulates without writing, prepares in an isolated worktree, and applies only the exact plan a person reviewed. |
| [`autonomous-remediation`](autonomous-remediation/) | Remediación autónoma que sabe cuándo no actuar: cuatro puertas que tienen que pasar todas, la vuelta atrás armada antes de aplicar y, ante la duda, un ticket preciso para una persona. | Autonomous remediation that knows when not to act: four gates that must all pass, a rollback armed before applying, and, when in doubt, a precise ticket for a person. |

## Cómo correr

No hay un `Cargo.toml` en la raíz: cada proyecto se compila y se prueba por separado.

```sh
git clone https://github.com/angelemontenegro18-crypto/systems-portfolio
cd systems-portfolio/<proyecto>
cargo test
```

Cada README explica qué garantiza su proyecto, qué prueba lo sostiene y cómo correr su
ejemplo.

## Plataforma

Probados con rustc 1.93 y 1.97. Los nueve pasan `cargo fmt --check` con la configuración por
defecto de rustfmt. Algunas pruebas de `session-isolation` leen `/proc` y solo corren en
Linux; en otros sistemas se omiten. Tres pruebas de `staged-agent` crean enlaces simbólicos y
solo corren en Unix, y las de `staged-agent` necesitan `git` en el `PATH`.

## Licencia

MIT — ver [LICENSE](LICENSE). Cada proyecto incluye además su propia copia.
