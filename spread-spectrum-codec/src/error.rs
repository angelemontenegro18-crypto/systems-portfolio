//! El error común de las operaciones del módem.

use core::fmt;

/// Lo que puede salir mal en una operación del módem. Ninguna función de la biblioteca entra
/// en pánico: todo camino de error termina aquí.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// El búfer de salida no alcanza para el resultado.
    SalidaCorta,
    /// La entrada no tiene un número entero de símbolos o de palabras.
    LargoNoMultiplo,
    /// Dos códigos que deberían tener el mismo largo no lo tienen.
    LargosDistintos,
    /// Un bit de entrada no es 0 ni 1.
    BitInvalido,
    /// Un valor fuera de su rango; por ejemplo, un nibble mayor que 15.
    FueraDeRango,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let texto = match self {
            Error::SalidaCorta => "el búfer de salida es demasiado corto",
            Error::LargoNoMultiplo => "la entrada no tiene un número entero de símbolos",
            Error::LargosDistintos => "los códigos tienen largos distintos",
            Error::BitInvalido => "un bit de entrada no es 0 ni 1",
            Error::FueraDeRango => "un valor está fuera de su rango",
        };
        f.write_str(texto)
    }
}
