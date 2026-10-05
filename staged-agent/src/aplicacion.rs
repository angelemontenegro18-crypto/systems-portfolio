//! Aplicar: llevar a la rama destino solo el plan exacto que se preparó.
//!
//! Todas las comprobaciones van antes de tocar nada, y cada una que falla deja
//! el repositorio como estaba:
//!
//! 1. la confirmación es el texto `aplicar <huella corta>` del plan revisado;
//! 2. la rama en uso es la destino del plan;
//! 3. el árbol de trabajo está limpio;
//! 4. la rama preparada es un solo commit sobre la punta actual de la destino;
//! 5. la huella **recalculada desde el contenido de la rama** coincide con la
//!    revisada. No se confía en lo que diga [`Preparado`]: si alguien cambió la
//!    rama después de prepararla, la huella ya no coincide.
//!
//! Recién entonces se avanza la rama destino (`merge --ff-only`).

use std::path::Path;

use crate::error::Error;
use crate::git::Git;
use crate::lectura;
use crate::plan::{huella_de_commit, texto_de_confirmacion, Huella};
use crate::preparacion::Preparado;
use crate::simulacion::validar_rama;

/// La confirmación explícita de una persona.
///
/// Tiene que ser el texto `aplicar <huella corta>` del plan revisado: una
/// confirmación de otro plan no sirve, y un «sí» genérico tampoco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmacion(String);

impl Confirmacion {
    /// Lo que la persona escribió.
    pub fn escrita(texto: impl Into<String>) -> Confirmacion {
        Confirmacion(texto.into())
    }
}

/// Resultado de aplicar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aplicado {
    /// El commit al que avanzó la rama destino.
    pub commit: String,
    /// Cuántos archivos cambiaron.
    pub archivos: usize,
}

pub(crate) fn aplicar(
    git: &Git,
    raiz: &Path,
    preparado: &Preparado,
    huella_revisada: &Huella,
    confirmacion: &Confirmacion,
) -> Result<Aplicado, Error> {
    if confirmacion.0.trim() != texto_de_confirmacion(huella_revisada) {
        return Err(Error::SinConfirmacion);
    }
    // Los nombres pueden venir de `Preparado::desde`: se validan antes de que
    // lleguen a `git`.
    validar_rama(&preparado.destino)?;
    validar_rama(&preparado.rama)?;

    let en_uso = lectura::rama_en_uso(git, raiz)?;
    if en_uso.as_deref() != Some(preparado.destino.as_str()) {
        return Err(Error::OtraRamaEnUso {
            esperada: preparado.destino.clone(),
            actual: en_uso,
        });
    }

    // `--no-optional-locks`: ni siquiera refrescar el índice, que también es
    // escribir.
    let estado = git.ejecutar(
        raiz,
        [
            "--no-optional-locks",
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
        ],
    )?;
    if !estado.is_empty() {
        let rutas = estado
            .split(|&b| b == 0)
            .filter(|r| r.len() > 3)
            .map(|r| String::from_utf8_lossy(&r[3..]).into_owned())
            .collect();
        return Err(Error::ArbolSucio(rutas));
    }

    let punta = lectura::commit_de_rama(git, raiz, &preparado.destino)?
        .ok_or_else(|| Error::RamaInexistente(preparado.destino.clone()))?;
    let commit = lectura::commit_de_rama(git, raiz, &preparado.rama)?
        .ok_or_else(|| Error::RamaInexistente(preparado.rama.clone()))?;
    let padres = lectura::padres(git, raiz, &commit)?;
    let [padre] = padres.as_slice() else {
        return Err(Error::PlanAlterado);
    };
    if *padre != punta {
        // Si la punta de la destino sigue en la historia de la rama preparada,
        // la que cambió es la rama preparada (commits agregados encima); si no,
        // la destino avanzó después de preparar.
        if lectura::es_ancestro(git, raiz, &punta, &commit)? {
            return Err(Error::PlanAlterado);
        }
        return Err(Error::DestinoMovido {
            esperado: padre.clone(),
            actual: punta,
        });
    }
    if huella_de_commit(git, raiz, &preparado.destino, &punta, &commit)? != Some(*huella_revisada) {
        return Err(Error::PlanAlterado);
    }
    let archivos = lectura::diferencias(git, raiz, &punta, &commit)?.len();

    git.ejecutar(raiz, ["merge", "--ff-only", "--quiet", commit.as_str()])?;
    let nueva_punta = lectura::commit_de_rama(git, raiz, &preparado.destino)?;
    if nueva_punta.as_deref() != Some(commit.as_str()) {
        return Err(Error::RespuestaInesperada(format!(
            "la rama destino quedó en {nueva_punta:?}"
        )));
    }
    // La rama preparada ya está contenida en la destino; si no se puede borrar,
    // no importa.
    let _ = git.ejecutar(raiz, ["branch", "-d", "--", preparado.rama.as_str()]);
    Ok(Aplicado { commit, archivos })
}
