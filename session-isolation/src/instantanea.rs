//! Instantánea generacional: un escritor, N lectores, cero locks para leer.
//!
//! Es un *seqlock*. El escritor pone la secuencia en impar, escribe, y la
//! devuelve a par; el lector lee la secuencia, copia los datos, vuelve a leer
//! la secuencia, y si cambió (o era impar) reintenta. Nunca se bloquea a un
//! lector y nunca se devuelve una mezcla de dos escrituras.
//!
//! ## Por qué la carga útil son palabras atómicas
//!
//! La versión ingenua guarda el valor en una celda normal y lo lee con un
//! puntero crudo mientras el escritor puede estar escribiendo. Aunque después
//! descarte la copia rota, esa lectura concurrente **ya es una carrera de
//! datos** — comportamiento indefinido en el modelo de memoria de Rust. Que en
//! x86 "funcione" no lo vuelve correcto.
//!
//! Acá cada palabra de la carga útil es un `AtomicU64` leído y escrito con
//! orden `Relaxed`, encuadrado por *fences* de adquisición y liberación (el
//! protocolo que describe Hans Boehm en *Can seqlocks get along with
//! programming language memory models?*). Leer una palabra a medio actualizar
//! está definido; la secuencia dice si hay que descartarla. Resultado: el
//! módulo no necesita `unsafe`, y el crate lo prohíbe.
//!
//! Un mutex serializa a los escritores: dos escritores concurrentes romperían
//! la paridad de la secuencia.

use std::marker::PhantomData;
use std::sync::atomic::{fence, AtomicU64, Ordering};
use std::sync::Mutex;

/// Un tipo que cabe en `N` palabras de 64 bits.
pub trait Codificable<const N: usize>: Sized {
    /// El valor como palabras.
    fn a_palabras(&self) -> [u64; N];
    /// El valor desde palabras producidas por [`Codificable::a_palabras`].
    fn desde_palabras(palabras: [u64; N]) -> Self;
}

/// Una lectura consistente, con la generación en la que se tomó.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instantanea<T> {
    /// Cuántas publicaciones hubo antes de esta lectura. Empieza en 0.
    pub generacion: u64,
    /// El valor publicado en esa generación.
    pub valor: T,
}

/// Valor compartido de publicación poco frecuente y lectura muy frecuente.
pub struct Publicador<T: Codificable<N>, const N: usize> {
    secuencia: AtomicU64,
    palabras: [AtomicU64; N],
    escritor: Mutex<()>,
    _tipo: PhantomData<fn() -> T>,
}

impl<T: Codificable<N>, const N: usize> Publicador<T, N> {
    /// Publica `inicial` como generación 0.
    pub fn nuevo(inicial: &T) -> Self {
        let p = inicial.a_palabras();
        Self {
            secuencia: AtomicU64::new(0),
            palabras: std::array::from_fn(|i| AtomicU64::new(p[i])),
            escritor: Mutex::new(()),
            _tipo: PhantomData,
        }
    }

    /// Publica un valor nuevo y devuelve su generación.
    pub fn publicar(&self, valor: &T) -> u64 {
        // Se codifica ANTES de abrir la ventana de escritura: si `a_palabras`
        // entrara en pánico con la secuencia impar, los lectores esperarían
        // para siempre. Dentro de la ventana solo hay stores atómicos.
        let nuevas = valor.a_palabras();

        // Nada puede entrar en pánico con el guardia tomado, así que un mutex
        // envenenado solo puede venir de afuera; se recupera y se sigue.
        let _unico = self.escritor.lock().unwrap_or_else(|e| e.into_inner());
        let s = self.secuencia.load(Ordering::Relaxed);

        self.secuencia.store(s + 1, Ordering::Relaxed); // impar: escritura en curso
        fence(Ordering::Release);
        for (celda, palabra) in self.palabras.iter().zip(nuevas) {
            celda.store(palabra, Ordering::Relaxed);
        }
        self.secuencia.store(s + 2, Ordering::Release); // par: estable

        (s + 2) / 2
    }

    /// Lee sin bloquear. Reintenta mientras haya una escritura en curso.
    pub fn leer(&self) -> Instantanea<T> {
        loop {
            let antes = self.secuencia.load(Ordering::Acquire);
            if antes & 1 == 0 {
                let mut copia = [0u64; N];
                for (destino, celda) in copia.iter_mut().zip(&self.palabras) {
                    *destino = celda.load(Ordering::Relaxed);
                }
                fence(Ordering::Acquire);
                if self.secuencia.load(Ordering::Relaxed) == antes {
                    return Instantanea {
                        generacion: antes / 2,
                        valor: T::desde_palabras(copia),
                    };
                }
            }
            std::hint::spin_loop();
        }
    }

    /// Generación publicada más reciente.
    pub fn generacion(&self) -> u64 {
        self.secuencia.load(Ordering::Acquire) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Par(u64, u64);

    impl Codificable<2> for Par {
        fn a_palabras(&self) -> [u64; 2] {
            [self.0, self.1]
        }
        fn desde_palabras(p: [u64; 2]) -> Self {
            Par(p[0], p[1])
        }
    }

    #[test]
    fn cada_publicacion_avanza_una_generacion() {
        let p = Publicador::nuevo(&Par(1, 2));
        assert_eq!(
            p.leer(),
            Instantanea {
                generacion: 0,
                valor: Par(1, 2)
            }
        );
        assert_eq!(p.publicar(&Par(3, 4)), 1);
        assert_eq!(p.publicar(&Par(5, 6)), 2);
        assert_eq!(
            p.leer(),
            Instantanea {
                generacion: 2,
                valor: Par(5, 6)
            }
        );
        assert_eq!(p.generacion(), 2);
    }
}
