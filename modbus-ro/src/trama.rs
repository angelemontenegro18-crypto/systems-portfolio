//! Codificación y validación de tramas Modbus TCP (MBAP + PDU).
//!
//! Solo se codifican **peticiones de lectura** y solo se decodifican sus
//! respuestas. La validación es estricta: una respuesta que no corresponde
//! exactamente a la petición enviada (otra transacción, otra unidad, otro
//! largo) se rechaza con un error que dice qué no cuadró, en vez de devolver
//! valores que parezcan buenos.

use std::fmt;

/// Largo de la cabecera MBAP: transacción (2) + protocolo (2) + largo (2) + unidad (1).
pub const LARGO_MBAP: usize = 7;

/// Largo total de una petición de lectura: MBAP + función + dirección + cantidad.
pub const LARGO_PETICION: usize = LARGO_MBAP + 5;

/// Máximo de registros por lectura que admite el protocolo (125 × 2 bytes = 250).
pub const MAX_REGISTROS: u16 = 125;

/// Largo máximo de un PDU Modbus.
const MAX_PDU: usize = 253;

/// Bit que el esclavo enciende en el código de función para señalar una excepción.
const BIT_EXCEPCION: u8 = 0x80;

/// Las únicas operaciones que este crate sabe expresar. Ambas leen.
///
/// No hay una tercera variante ni un constructor desde un `u8`: un código de
/// función que no esté acá no se puede pedir, por construcción.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Funcion {
    /// Lectura de registros de retención (código 3).
    RegistrosRetencion,
    /// Lectura de registros de entrada (código 4).
    RegistrosEntrada,
}

impl Funcion {
    /// Código de función en el cable. Es el único lugar del crate que produce uno.
    pub const fn codigo(self) -> u8 {
        match self {
            Funcion::RegistrosRetencion => 0x03,
            Funcion::RegistrosEntrada => 0x04,
        }
    }

    /// Nombre legible, para registros y paneles.
    pub const fn nombre(self) -> &'static str {
        match self {
            Funcion::RegistrosRetencion => "registros de retención",
            Funcion::RegistrosEntrada => "registros de entrada",
        }
    }
}

/// Una petición de lectura ya validada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peticion {
    transaccion: u16,
    unidad: u8,
    funcion: Funcion,
    direccion: u16,
    cantidad: u16,
}

impl Peticion {
    /// Arma una petición. Rechaza una cantidad fuera de `1..=125` y un rango
    /// que se pase del final del espacio de direcciones.
    pub fn nueva(
        transaccion: u16,
        unidad: u8,
        funcion: Funcion,
        direccion: u16,
        cantidad: u16,
    ) -> Result<Self, &'static str> {
        if cantidad == 0 || cantidad > MAX_REGISTROS {
            return Err("la cantidad de registros tiene que estar entre 1 y 125");
        }
        if u32::from(direccion) + u32::from(cantidad) > 65_536 {
            return Err("el rango pedido se pasa del final del espacio de direcciones");
        }
        Ok(Self { transaccion, unidad, funcion, direccion, cantidad })
    }

    /// Identificador de transacción.
    pub fn transaccion(&self) -> u16 {
        self.transaccion
    }

    /// Unidad (esclavo) a la que va dirigida.
    pub fn unidad(&self) -> u8 {
        self.unidad
    }

    /// Qué se lee.
    pub fn funcion(&self) -> Funcion {
        self.funcion
    }

    /// Primer registro.
    pub fn direccion(&self) -> u16 {
        self.direccion
    }

    /// Cuántos registros.
    pub fn cantidad(&self) -> u16 {
        self.cantidad
    }

    /// La petición en bytes, lista para el cable.
    pub fn codificar(&self) -> [u8; LARGO_PETICION] {
        let tx = self.transaccion.to_be_bytes();
        let dir = self.direccion.to_be_bytes();
        let cant = self.cantidad.to_be_bytes();
        // largo = unidad (1) + PDU (5)
        [
            tx[0], tx[1], 0, 0, 0, 6, self.unidad,
            self.funcion.codigo(), dir[0], dir[1], cant[0], cant[1],
        ]
    }
}

/// Todo lo que puede estar mal en una respuesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorTrama {
    /// Llegaron menos bytes de los que la cabecera anuncia.
    Truncada {
        /// Bytes esperados.
        esperados: usize,
        /// Bytes recibidos.
        recibidos: usize,
    },
    /// Llegaron más bytes de los que la respuesta admite.
    SobranBytes {
        /// Bytes esperados.
        esperados: usize,
        /// Bytes recibidos.
        recibidos: usize,
    },
    /// El identificador de protocolo no es 0 (no es Modbus).
    Protocolo(u16),
    /// El campo de largo de la cabecera está fuera de lo que admite Modbus.
    LargoFueraDeRango(u16),
    /// La respuesta pertenece a otra transacción.
    OtraTransaccion {
        /// La que se envió.
        esperada: u16,
        /// La que volvió.
        recibida: u16,
    },
    /// La respuesta viene de otra unidad.
    OtraUnidad {
        /// La que se consultó.
        esperada: u8,
        /// La que contestó.
        recibida: u8,
    },
    /// El código de función de la respuesta no corresponde a la petición.
    FuncionInesperada {
        /// El que se envió.
        esperada: u8,
        /// El que volvió.
        recibida: u8,
    },
    /// El equipo respondió con una excepción Modbus.
    Excepcion {
        /// Código de excepción.
        codigo: u8,
    },
    /// El conteo de bytes no corresponde a la cantidad de registros pedida.
    ConteoDeBytes {
        /// Bytes esperados de datos.
        esperado: usize,
        /// Conteo que anunció la respuesta.
        anunciado: u8,
    },
}

impl ErrorTrama {
    /// Nombre de una excepción Modbus estándar.
    pub fn nombre_excepcion(codigo: u8) -> &'static str {
        match codigo {
            1 => "función no admitida por el equipo",
            2 => "dirección fuera del mapa del equipo",
            3 => "valor no admitido en la petición",
            4 => "falla interna del equipo",
            5 => "petición aceptada, todavía en proceso",
            6 => "equipo ocupado",
            8 => "error de paridad en la memoria del equipo",
            10 => "el gateway no tiene ruta al destino",
            11 => "el destino detrás del gateway no respondió",
            _ => "excepción no estándar",
        }
    }
}

impl fmt::Display for ErrorTrama {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncada { esperados, recibidos } => {
                write!(f, "trama truncada: se esperaban {esperados} bytes y llegaron {recibidos}")
            }
            Self::SobranBytes { esperados, recibidos } => {
                write!(f, "sobran bytes: se esperaban {esperados} y llegaron {recibidos}")
            }
            Self::Protocolo(p) => write!(f, "identificador de protocolo {p}: no es Modbus"),
            Self::LargoFueraDeRango(l) => write!(f, "campo de largo {l} fuera de rango"),
            Self::OtraTransaccion { esperada, recibida } => {
                write!(f, "respuesta de otra transacción: se esperaba {esperada} y llegó {recibida}")
            }
            Self::OtraUnidad { esperada, recibida } => {
                write!(f, "respuesta de otra unidad: se consultó {esperada} y contestó {recibida}")
            }
            Self::FuncionInesperada { esperada, recibida } => {
                write!(f, "función inesperada: se envió {esperada} y volvió {recibida}")
            }
            Self::Excepcion { codigo } => {
                write!(f, "excepción Modbus {codigo}: {}", Self::nombre_excepcion(*codigo))
            }
            Self::ConteoDeBytes { esperado, anunciado } => {
                write!(f, "conteo de bytes {anunciado}, se esperaban {esperado}")
            }
        }
    }
}

impl std::error::Error for ErrorTrama {}

/// Cuántos bytes de PDU siguen a una cabecera MBAP ya leída.
///
/// Sirve para leer del socket exactamente lo que corresponde, ni un byte más.
pub fn largo_pdu(cabecera: &[u8; LARGO_MBAP]) -> Result<usize, ErrorTrama> {
    let protocolo = u16::from_be_bytes([cabecera[2], cabecera[3]]);
    if protocolo != 0 {
        return Err(ErrorTrama::Protocolo(protocolo));
    }
    let largo = u16::from_be_bytes([cabecera[4], cabecera[5]]);
    // El largo cuenta la unidad (1 byte) más el PDU, que mide entre 2 y 253.
    let pdu = usize::from(largo).checked_sub(1).ok_or(ErrorTrama::LargoFueraDeRango(largo))?;
    if !(2..=MAX_PDU).contains(&pdu) {
        return Err(ErrorTrama::LargoFueraDeRango(largo));
    }
    Ok(pdu)
}

/// Valida una respuesta completa (MBAP + PDU) contra la petición que la
/// originó y devuelve los registros leídos.
pub fn decodificar_respuesta(peticion: &Peticion, trama: &[u8]) -> Result<Vec<u16>, ErrorTrama> {
    let cabecera: &[u8; LARGO_MBAP] = trama
        .get(..LARGO_MBAP)
        .and_then(|c| c.try_into().ok())
        .ok_or(ErrorTrama::Truncada { esperados: LARGO_MBAP, recibidos: trama.len() })?;

    let pdu_largo = largo_pdu(cabecera)?;
    let total = LARGO_MBAP + pdu_largo;
    if trama.len() < total {
        return Err(ErrorTrama::Truncada { esperados: total, recibidos: trama.len() });
    }
    if trama.len() > total {
        return Err(ErrorTrama::SobranBytes { esperados: total, recibidos: trama.len() });
    }

    let transaccion = u16::from_be_bytes([cabecera[0], cabecera[1]]);
    if transaccion != peticion.transaccion {
        return Err(ErrorTrama::OtraTransaccion { esperada: peticion.transaccion, recibida: transaccion });
    }
    if cabecera[6] != peticion.unidad {
        return Err(ErrorTrama::OtraUnidad { esperada: peticion.unidad, recibida: cabecera[6] });
    }

    let pdu = &trama[LARGO_MBAP..];
    let esperada = peticion.funcion.codigo();
    let recibida = pdu[0];

    if recibida == esperada | BIT_EXCEPCION {
        if pdu.len() != 2 {
            return Err(ErrorTrama::SobranBytes { esperados: LARGO_MBAP + 2, recibidos: trama.len() });
        }
        return Err(ErrorTrama::Excepcion { codigo: pdu[1] });
    }
    if recibida != esperada {
        return Err(ErrorTrama::FuncionInesperada { esperada, recibida });
    }

    let datos_esperados = usize::from(peticion.cantidad) * 2;
    let anunciado = pdu[1];
    if usize::from(anunciado) != datos_esperados {
        return Err(ErrorTrama::ConteoDeBytes { esperado: datos_esperados, anunciado });
    }
    if pdu.len() != 2 + datos_esperados {
        return Err(ErrorTrama::Truncada { esperados: LARGO_MBAP + 2 + datos_esperados, recibidos: trama.len() });
    }

    Ok(pdu[2..].chunks_exact(2).map(|par| u16::from_be_bytes([par[0], par[1]])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peticion() -> Peticion {
        Peticion::nueva(0x1234, 7, Funcion::RegistrosRetencion, 100, 2).expect("petición válida")
    }

    /// Respuesta armada byte a byte, sin pasar por el codificador que se prueba.
    fn respuesta(tx: u16, unidad: u8, pdu: &[u8]) -> Vec<u8> {
        let largo = u16::try_from(pdu.len() + 1).expect("pdu chico");
        let mut v = Vec::new();
        v.extend_from_slice(&tx.to_be_bytes());
        v.extend_from_slice(&[0, 0]);
        v.extend_from_slice(&largo.to_be_bytes());
        v.push(unidad);
        v.extend_from_slice(pdu);
        v
    }

    #[test]
    fn la_peticion_se_codifica_como_dice_la_especificacion() {
        // Leer 2 registros de retención desde el 100 en la unidad 7.
        // Transacción 0x1234 · protocolo 0 · largo 6 · unidad 7 · función 3 ·
        // dirección 100 · cantidad 2. En decimal donde un literal hexadecimal
        // se confundiría con un código de escritura (ver tests/solo_lectura.rs).
        assert_eq!(peticion().codificar(), [0x12, 0x34, 0, 0, 0, 6, 7, 3, 0, 100, 0, 2]);
    }

    #[test]
    fn una_respuesta_correcta_devuelve_los_registros() {
        let r = respuesta(0x1234, 7, &[0x03, 4, 0x00, 0x2A, 0xFF, 0xFE]);
        assert_eq!(decodificar_respuesta(&peticion(), &r), Ok(vec![42, 65_534]));
    }

    #[test]
    fn una_excepcion_se_reporta_con_su_codigo() {
        let r = respuesta(0x1234, 7, &[0x83, 2]);
        assert_eq!(decodificar_respuesta(&peticion(), &r), Err(ErrorTrama::Excepcion { codigo: 2 }));
    }

    #[test]
    fn cada_desajuste_tiene_su_propio_error() {
        let p = peticion();
        let buena = [0x03, 4, 0, 1, 0, 2];
        assert!(matches!(
            decodificar_respuesta(&p, &respuesta(0x9999, 7, &buena)),
            Err(ErrorTrama::OtraTransaccion { .. })
        ));
        assert!(matches!(
            decodificar_respuesta(&p, &respuesta(0x1234, 8, &buena)),
            Err(ErrorTrama::OtraUnidad { .. })
        ));
        assert!(matches!(
            decodificar_respuesta(&p, &respuesta(0x1234, 7, &[0x04, 4, 0, 1, 0, 2])),
            Err(ErrorTrama::FuncionInesperada { .. })
        ));
        assert!(matches!(
            decodificar_respuesta(&p, &respuesta(0x1234, 7, &[0x03, 6, 0, 1, 0, 2, 0, 3])),
            Err(ErrorTrama::ConteoDeBytes { .. })
        ));
        let mut corta = respuesta(0x1234, 7, &buena);
        corta.pop();
        assert!(matches!(decodificar_respuesta(&p, &corta), Err(ErrorTrama::Truncada { .. })));
        let mut larga = respuesta(0x1234, 7, &buena);
        larga.push(0);
        assert!(matches!(decodificar_respuesta(&p, &larga), Err(ErrorTrama::SobranBytes { .. })));
    }

    #[test]
    fn la_cabecera_rechaza_protocolo_y_largo_invalidos() {
        assert_eq!(largo_pdu(&[0, 1, 0, 1, 0, 6, 1]), Err(ErrorTrama::Protocolo(1)));
        assert_eq!(largo_pdu(&[0, 1, 0, 0, 0, 1, 1]), Err(ErrorTrama::LargoFueraDeRango(1)));
        assert_eq!(largo_pdu(&[0, 1, 0, 0, 1, 0, 1]), Err(ErrorTrama::LargoFueraDeRango(256)));
        assert_eq!(largo_pdu(&[0, 1, 0, 0, 0, 7, 1]), Ok(6));
    }

    #[test]
    fn la_peticion_valida_cantidad_y_rango() {
        assert!(Peticion::nueva(1, 1, Funcion::RegistrosEntrada, 0, 0).is_err());
        assert!(Peticion::nueva(1, 1, Funcion::RegistrosEntrada, 0, 126).is_err());
        assert!(Peticion::nueva(1, 1, Funcion::RegistrosEntrada, 65_535, 2).is_err());
        assert!(Peticion::nueva(1, 1, Funcion::RegistrosEntrada, 65_535, 1).is_ok());
        assert!(Peticion::nueva(1, 1, Funcion::RegistrosEntrada, 0, 125).is_ok());
    }
}
