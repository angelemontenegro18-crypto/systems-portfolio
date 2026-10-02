//! La clave de sellado.

use std::fmt;

use zeroize::Zeroizing;

use crate::ErrorSello;

/// Clave de 256 bits. Se borra de la memoria al soltarse, y su `Debug` no la muestra.
#[derive(Clone)]
pub struct Clave(Zeroizing<[u8; 32]>);

impl Clave {
    /// Una clave a partir de 32 bytes. Quien la tenga guardada es responsable
    /// de protegerla; este crate no deriva claves de contraseñas.
    pub fn desde_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Una clave nueva, del generador aleatorio del sistema operativo. El error
    /// es propio del crate: la API no expone tipos de sus dependencias.
    pub fn generar() -> Result<Self, ErrorSello> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(bytes.as_mut()).map_err(|_| ErrorSello::Entropia)?;
        Ok(Self(bytes))
    }

    pub(crate) fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Clave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Clave(oculta)")
    }
}
