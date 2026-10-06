//! El lado del respaldo: la cerca de época y la confirmación en dos fases.
//!
//! Un [`Receptor`] tiene un área de preparación propia, de `CAP` bytes, adonde
//! [`cargar`](Receptor::cargar) copia cada paquete, y recuerda la última época que confirmó.
//!
//! 1. [`preparar`](Receptor::preparar) verifica el paquete cargado y le aplica la **cerca**:
//!    solo pasa una época **estrictamente mayor** que la última confirmada. Una repetición o
//!    un estado viejo se rechazan. Si pasa, entrega un [`Preparado`], que deja leer el
//!    contenido para instalarlo.
//! 2. [`confirmar`](Preparado::confirmar) registra la época y **borra el área** con
//!    `zeroize`, que el compilador no puede eliminar por inútil. El borrado se ve desde la
//!    API: [`area_de_preparacion`](Receptor::area_de_preparacion) queda en ceros.
//! 3. [`abortar`](Preparado::abortar) —o soltar el `Preparado` sin confirmar— no consume la
//!    época y deja el área como estaba: el mismo paquete se puede volver a preparar.
//!
//! Las épocas válidas empiezan en 1: un receptor nuevo no confirmó ninguna (su última época es
//! 0).
//!
//! Mientras hay un `Preparado`, el receptor está prestado: no se puede cargar otro paquete
//! encima.
//!
//! ```compile_fail,E0499
//! use state_handoff::paquete::sellar;
//! use state_handoff::receptor::Receptor;
//!
//! let mut bytes = [0u8; 64];
//! let n = sellar(&[1, 2, 3], 1, 2, 5, &mut bytes).unwrap();
//! let mut respaldo = Receptor::<64>::nuevo(2);
//! respaldo.cargar(&bytes[..n]).unwrap();
//! let preparado = respaldo.preparar().unwrap();
//! // error[E0499]: el receptor ya está prestado al preparado.
//! respaldo.cargar(&bytes[..n]).unwrap();
//! preparado.confirmar();
//! ```
//!
//! Y un preparado se confirma una sola vez: `confirmar` lo consume.
//!
//! ```compile_fail,E0382
//! use state_handoff::paquete::sellar;
//! use state_handoff::receptor::Receptor;
//!
//! let mut bytes = [0u8; 64];
//! let n = sellar(&[1, 2, 3], 1, 2, 5, &mut bytes).unwrap();
//! let mut respaldo = Receptor::<64>::nuevo(2);
//! respaldo.cargar(&bytes[..n]).unwrap();
//! let preparado = respaldo.preparar().unwrap();
//! preparado.confirmar();
//! // error[E0382]: `preparado` ya se consumió.
//! preparado.confirmar();
//! ```

use zeroize::Zeroize;

use crate::cabecera::LARGO_CABECERA;
use crate::paquete::Paquete;
use crate::Error;

/// El lado del respaldo, con un área de preparación de `CAP` bytes.
#[derive(Debug)]
pub struct Receptor<const CAP: usize> {
    propio: u16,
    ultima_epoca: u64,
    area: [u8; CAP],
    cargados: usize,
}

impl<const CAP: usize> Receptor<CAP> {
    /// Un receptor para el nodo `propio`, que todavía no confirmó ninguna época.
    pub const fn nuevo(propio: u16) -> Receptor<CAP> {
        Receptor::con_epoca(propio, 0)
    }

    /// Un receptor que ya confirmó hasta `ultima_epoca`; por ejemplo, la que guardó antes de
    /// reiniciarse.
    pub const fn con_epoca(propio: u16, ultima_epoca: u64) -> Receptor<CAP> {
        Receptor {
            propio,
            ultima_epoca,
            area: [0; CAP],
            cargados: 0,
        }
    }

    /// La última época confirmada (0 si ninguna).
    pub fn ultima_epoca(&self) -> u64 {
        self.ultima_epoca
    }

    /// El área de preparación entera, para mirarla. Después de confirmar está en ceros.
    pub fn area_de_preparacion(&self) -> &[u8; CAP] {
        &self.area
    }

    /// Copia `bytes` al área de preparación y borra lo que quedaba detrás de una carga
    /// anterior. Si no caben, [`Error::NoCabe`] y el área no cambia.
    pub fn cargar(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let destino = self.area.get_mut(..bytes.len()).ok_or(Error::NoCabe)?;
        destino.copy_from_slice(bytes);
        if let Some(cola) = self.area.get_mut(bytes.len()..) {
            cola.zeroize();
        }
        self.cargados = bytes.len();
        Ok(())
    }

    /// Verifica el paquete cargado y le aplica la cerca de época. Si pasa, entrega el
    /// [`Preparado`]; si no, el error, y nada cambia.
    pub fn preparar(&mut self) -> Result<Preparado<'_, CAP>, Error> {
        let bytes = self.area.get(..self.cargados).ok_or(Error::Truncado)?;
        let paquete = Paquete::leer(bytes)?.verificar(self.propio)?;
        let (epoca, largo) = (paquete.epoca(), paquete.contenido().len());
        if epoca <= self.ultima_epoca {
            return Err(Error::EpocaVieja {
                ultima: self.ultima_epoca,
                recibida: epoca,
            });
        }
        Ok(Preparado {
            receptor: self,
            epoca,
            largo,
        })
    }
}

/// Un paquete verificado que pasó la cerca, esperando que se lo confirme o se lo aborte.
#[derive(Debug)]
pub struct Preparado<'r, const CAP: usize> {
    receptor: &'r mut Receptor<CAP>,
    epoca: u64,
    largo: usize,
}

impl<const CAP: usize> Preparado<'_, CAP> {
    /// La época que trae.
    pub fn epoca(&self) -> u64 {
        self.epoca
    }

    /// El contenido verificado. Hay que usarlo —instalarlo, copiarlo— antes de confirmar:
    /// confirmar lo borra.
    pub fn contenido(&self) -> &[u8] {
        self.receptor
            .area
            .get(LARGO_CABECERA..LARGO_CABECERA + self.largo)
            .unwrap_or(&[])
    }

    /// Confirma: registra la época como la última aceptada y borra el área de preparación.
    pub fn confirmar(self) {
        self.receptor.ultima_epoca = self.epoca;
        self.receptor.area.zeroize();
        self.receptor.cargados = 0;
    }

    /// Aborta: la época no se consume y el área queda como estaba. Soltar el `Preparado` sin
    /// confirmar hace lo mismo; este método solo lo dice en voz alta.
    pub fn abortar(self) {}
}
