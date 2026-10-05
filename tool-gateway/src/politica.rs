//! Qué nunca se expone a un modelo, y por qué.
//!
//! La regla general: una herramienta expuesta tiene que ser una **función
//! pura** — sin E/S, sin estado, sin aleatoriedad, sin efectos. Estos nombres
//! quedan fuera aunque alguien los implemente, porque lo que hacen contradice
//! esa regla. El registro se niega a aceptarlos, y un test lo verifica.
//!
//! Los nombres son de ejemplo: representan las categorías de riesgo típicas de
//! un agente con herramientas, no un catálogo real.

/// Una herramienta vetada y el motivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vetada {
    /// Nombre que no se puede registrar.
    pub nombre: &'static str,
    /// Por qué.
    pub motivo: &'static str,
}

/// La lista de exclusión.
pub const EXCLUIDAS: &[Vetada] = &[
    Vetada {
        nombre: "ejecutar_comando",
        motivo: "lanza procesos: un efecto fuera del gateway y sin límite",
    },
    Vetada {
        nombre: "leer_archivo",
        motivo: "E/S de disco: puede exfiltrar cualquier archivo legible",
    },
    Vetada {
        nombre: "escribir_archivo",
        motivo: "E/S de disco con efectos persistentes",
    },
    Vetada {
        nombre: "consultar_url",
        motivo: "E/S de red: abre la puerta a SSRF y a exfiltración",
    },
    Vetada {
        nombre: "enviar_correo",
        motivo: "efecto externo e irreversible en nombre del usuario",
    },
    Vetada {
        nombre: "leer_variable_entorno",
        motivo: "el entorno suele guardar credenciales",
    },
    Vetada {
        nombre: "numero_aleatorio",
        motivo: "no determinista: la misma llamada da resultados distintos",
    },
    Vetada {
        nombre: "hora_actual",
        motivo: "no determinista y filtra información del entorno",
    },
];

/// El motivo del veto, si `nombre` está vetado.
pub fn motivo_de_veto(nombre: &str) -> Option<&'static str> {
    EXCLUIDAS
        .iter()
        .find(|v| v.nombre == nombre)
        .map(|v| v.motivo)
}
