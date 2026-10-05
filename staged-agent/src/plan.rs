//! El plan: qué cambiaría, en qué archivos, sobre qué commit, y su huella.

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::Error;
use crate::git::Git;
use crate::lectura;
use crate::rutas::RutaRelativa;

/// SHA-256 de un plan. Identifica exactamente qué se revisó.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Huella([u8; 32]);

impl Huella {
    /// Los primeros 12 dígitos hexadecimales: lo que va en el nombre de la rama
    /// y en el texto de confirmación.
    pub fn corta(&self) -> String {
        self.to_string()[..12].to_string()
    }

    /// Lee una huella completa en hexadecimal (64 dígitos).
    pub fn desde_hex(texto: &str) -> Option<Huella> {
        if texto.len() != 64 || !texto.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&texto[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Huella(bytes))
    }

    /// Los 32 bytes.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Huella {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Huella {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Huella({self})")
    }
}

/// Un archivo que el plan modifica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cambio {
    pub(crate) ruta: RutaRelativa,
    pub(crate) reglas: Vec<&'static str>,
    pub(crate) antes: Vec<u8>,
    pub(crate) despues: Vec<u8>,
}

impl Cambio {
    /// Qué archivo.
    pub fn ruta(&self) -> &RutaRelativa {
        &self.ruta
    }

    /// Qué reglas lo modificaron, en orden.
    pub fn reglas(&self) -> &[&'static str] {
        &self.reglas
    }

    /// El contenido en el commit base.
    pub fn antes(&self) -> &[u8] {
        &self.antes
    }

    /// El contenido propuesto.
    pub fn despues(&self) -> &[u8] {
        &self.despues
    }
}

/// Por qué un archivo quedó fuera del plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Omision {
    /// No es texto UTF-8, o tiene bytes nulos.
    Binario,
    /// Es un enlace simbólico: modificarlo tocaría otro archivo.
    EnlaceSimbolico,
    /// Es un submódulo.
    Submodulo,
    /// Su ruta no pasa la validación de [`RutaRelativa`].
    RutaInsegura,
    /// Otro tipo de entrada.
    Otro,
}

/// Un archivo que el plan no toca, y por qué.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Omitido {
    /// La ruta, tal como la da el repositorio.
    pub ruta: String,
    /// El motivo.
    pub motivo: Omision,
}

/// Lo que el agente haría sobre una rama, calculado sin escribir nada.
#[derive(Debug, Clone)]
pub struct Plan {
    pub(crate) destino: String,
    pub(crate) base: String,
    pub(crate) cambios: Vec<Cambio>,
    pub(crate) omitidos: Vec<Omitido>,
    pub(crate) huella: Huella,
}

impl Plan {
    pub(crate) fn nuevo(
        destino: String,
        base: String,
        mut cambios: Vec<Cambio>,
        omitidos: Vec<Omitido>,
    ) -> Plan {
        cambios.sort_by(|a, b| a.ruta.cmp(&b.ruta));
        let huella = calcular_huella(
            &destino,
            &base,
            cambios
                .iter()
                .map(|c| (&c.ruta, &c.antes[..], &c.despues[..])),
        );
        Plan {
            destino,
            base,
            cambios,
            omitidos,
            huella,
        }
    }

    /// La huella: el SHA-256 de la rama destino, el commit base y, por cada
    /// archivo, su ruta y los SHA-256 del antes y el después.
    pub fn huella(&self) -> Huella {
        self.huella
    }

    /// La rama sobre la que se simuló.
    pub fn destino(&self) -> &str {
        &self.destino
    }

    /// El commit de la rama destino al simular.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Los archivos que cambian, ordenados por ruta.
    pub fn cambios(&self) -> &[Cambio] {
        &self.cambios
    }

    /// Los archivos que se dejaron de lado.
    pub fn omitidos(&self) -> &[Omitido] {
        &self.omitidos
    }

    /// `true` si no hay nada que cambiar.
    pub fn esta_vacio(&self) -> bool {
        self.cambios.is_empty()
    }

    /// El texto que [`crate::Agente::aplicar`] exige como confirmación.
    pub fn confirmacion_esperada(&self) -> String {
        texto_de_confirmacion(&self.huella)
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "plan {} sobre {} ({})",
            self.huella.corta(),
            self.destino,
            &self.base[..12.min(self.base.len())]
        )?;
        for c in &self.cambios {
            writeln!(f, "  M {}  ({})", c.ruta, c.reglas.join(", "))?;
        }
        for o in &self.omitidos {
            writeln!(f, "  - {}  (omitido: {:?})", o.ruta, o.motivo)?;
        }
        if self.cambios.is_empty() {
            writeln!(f, "  nada que cambiar")?;
        }
        Ok(())
    }
}

/// `aplicar <huella corta>`.
pub(crate) fn texto_de_confirmacion(huella: &Huella) -> String {
    format!("aplicar {}", huella.corta())
}

/// La huella de un plan. Cada campo de largo variable va precedido por su
/// largo, así que dos planes distintos no pueden codificarse igual.
pub(crate) fn calcular_huella<'a>(
    destino: &str,
    base: &str,
    cambios: impl Iterator<Item = (&'a RutaRelativa, &'a [u8], &'a [u8])>,
) -> Huella {
    let mut resumen = Sha256::new();
    let mut campo = |bytes: &[u8]| {
        resumen.update((bytes.len() as u64).to_le_bytes());
        resumen.update(bytes);
    };
    campo(b"staged-agent/plan/v1");
    campo(destino.as_bytes());
    campo(base.as_bytes());
    for (ruta, antes, despues) in cambios {
        campo(ruta.como_str().as_bytes());
        campo(&Sha256::digest(antes));
        campo(&Sha256::digest(despues));
    }
    Huella(resumen.finalize().into())
}

/// La huella del plan que reproduce `commit` sobre `base`, recalculada desde el
/// repositorio; `None` si el commit no tiene forma de plan.
///
/// Un plan solo modifica el contenido de archivos regulares que ya existían:
/// un archivo nuevo, uno borrado, un cambio de modo o una ruta insegura
/// significan que lo que hay en el commit no salió de un plan.
pub(crate) fn huella_de_commit(
    git: &Git,
    raiz: &Path,
    destino: &str,
    base: &str,
    commit: &str,
) -> Result<Option<Huella>, Error> {
    let mut cambios = Vec::new();
    for d in lectura::diferencias(git, raiz, base, commit)? {
        let regular = matches!(d.modo_antes.as_str(), "100644" | "100755");
        if d.estado != "M" || d.modo_antes != d.modo_despues || !regular {
            return Ok(None);
        }
        let Some(ruta) = std::str::from_utf8(&d.ruta)
            .ok()
            .and_then(|t| RutaRelativa::nueva(t).ok())
        else {
            return Ok(None);
        };
        let antes = lectura::blob(git, raiz, &d.objeto_antes)?;
        let despues = lectura::blob(git, raiz, &d.objeto_despues)?;
        cambios.push((ruta, antes, despues));
    }
    cambios.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some(calcular_huella(
        destino,
        base,
        cambios.iter().map(|(r, a, d)| (r, &a[..], &d[..])),
    )))
}
