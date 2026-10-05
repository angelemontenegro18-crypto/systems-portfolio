//! Consultas de solo lectura al repositorio.
//!
//! Todo lo que la simulación sabe del repositorio pasa por aquí, y aquí solo
//! hay subcomandos de `git` que no escriben: `rev-parse`, `ls-tree`, `cat-file`,
//! `diff-tree` y `merge-base`. El test `solo_lectura` lo comprueba sobre el código fuente:
//! cualquier otra cadena literal en este archivo lo hace fallar.

use std::path::Path;

use crate::error::Error;
use crate::git::{ErrorGit, Git};

/// Un archivo de un commit, tal como lo lista `ls-tree`.
#[derive(Debug)]
pub(crate) struct Entrada {
    pub(crate) modo: String,
    pub(crate) tipo: String,
    pub(crate) objeto: String,
    pub(crate) ruta: Vec<u8>,
}

/// Un archivo que cambia entre dos commits, tal como lo lista `diff-tree`.
#[derive(Debug)]
pub(crate) struct Diferencia {
    pub(crate) modo_antes: String,
    pub(crate) modo_despues: String,
    pub(crate) objeto_antes: String,
    pub(crate) objeto_despues: String,
    pub(crate) estado: String,
    pub(crate) ruta: Vec<u8>,
}

/// `true` si `texto` es un identificador de objeto: solo hexadecimal, del largo
/// de SHA-1 o de SHA-256. Lo que no lo es no llega a la línea de comandos.
pub(crate) fn es_objeto(texto: &str) -> bool {
    (texto.len() == 40 || texto.len() == 64) && texto.bytes().all(|b| b.is_ascii_hexdigit())
}

fn objeto(texto: &str) -> Result<String, Error> {
    if es_objeto(texto) {
        Ok(texto.to_string())
    } else {
        Err(Error::RespuestaInesperada(texto.to_string()))
    }
}

/// El commit al que apunta `refs/heads/<rama>`, o `None` si la rama no existe.
pub(crate) fn commit_de_rama(git: &Git, raiz: &Path, rama: &str) -> Result<Option<String>, Error> {
    let referencia = format!("refs/heads/{rama}^{{commit}}");
    match git.ejecutar(
        raiz,
        ["rev-parse", "--verify", "--quiet", referencia.as_str()],
    ) {
        Ok(salida) => Ok(Some(objeto(String::from_utf8_lossy(&salida).trim())?)),
        // Con `--quiet`, una referencia inexistente sale con 1 y sin mensaje.
        Err(ErrorGit::Fallo {
            codigo: Some(1),
            mensaje,
            ..
        }) if mensaje.is_empty() => Ok(None),
        Err(e) => Err(Error::Git(e)),
    }
}

/// La referencia completa de la rama en uso (`refs/heads/...`), o `None` si
/// `HEAD` no apunta a una rama.
pub(crate) fn rama_en_uso(git: &Git, raiz: &Path) -> Result<Option<String>, Error> {
    let salida = git.ejecutar(raiz, ["rev-parse", "--symbolic-full-name", "HEAD"])?;
    let referencia = String::from_utf8_lossy(&salida).trim().to_string();
    Ok(referencia.strip_prefix("refs/heads/").map(str::to_string))
}

/// Los padres de un commit.
pub(crate) fn padres(git: &Git, raiz: &Path, commit: &str) -> Result<Vec<String>, Error> {
    let commit = objeto(commit)?;
    let consulta = format!("{commit}^@");
    let salida = git.ejecutar(raiz, ["rev-parse", consulta.as_str()])?;
    String::from_utf8_lossy(&salida)
        .lines()
        .map(objeto)
        .collect()
}

/// `true` si `ancestro` está en la historia de `commit`.
pub(crate) fn es_ancestro(
    git: &Git,
    raiz: &Path,
    ancestro: &str,
    commit: &str,
) -> Result<bool, Error> {
    let (ancestro, commit) = (objeto(ancestro)?, objeto(commit)?);
    match git.ejecutar(
        raiz,
        [
            "merge-base",
            "--is-ancestor",
            ancestro.as_str(),
            commit.as_str(),
        ],
    ) {
        Ok(_) => Ok(true),
        // `--is-ancestor` responde «no» con el código 1.
        Err(ErrorGit::Fallo {
            codigo: Some(1), ..
        }) => Ok(false),
        Err(e) => Err(Error::Git(e)),
    }
}

/// Todos los archivos de un commit, recursivamente.
pub(crate) fn archivos(git: &Git, raiz: &Path, commit: &str) -> Result<Vec<Entrada>, Error> {
    let commit = objeto(commit)?;
    let salida = git.ejecutar(
        raiz,
        ["ls-tree", "-r", "-z", "--full-tree", commit.as_str()],
    )?;
    let mut entradas = Vec::new();
    // Cada entrada: "<modo> <tipo> <objeto>\t<ruta>\0".
    for registro in salida.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let tab = registro
            .iter()
            .position(|&b| b == b'\t')
            .ok_or_else(|| inesperada(registro))?;
        let cabecera = String::from_utf8_lossy(&registro[..tab]);
        let partes: Vec<&str> = cabecera.split(' ').collect();
        let [modo, tipo, id] = partes[..] else {
            return Err(inesperada(registro));
        };
        entradas.push(Entrada {
            modo: modo.to_string(),
            tipo: tipo.to_string(),
            objeto: objeto(id)?,
            ruta: registro[tab + 1..].to_vec(),
        });
    }
    Ok(entradas)
}

/// El contenido de un blob, byte a byte.
pub(crate) fn blob(git: &Git, raiz: &Path, id: &str) -> Result<Vec<u8>, Error> {
    let id = objeto(id)?;
    Ok(git.ejecutar(raiz, ["cat-file", "blob", id.as_str()])?)
}

/// Los archivos que difieren entre dos commits, sin detectar renombres.
pub(crate) fn diferencias(
    git: &Git,
    raiz: &Path,
    de: &str,
    a: &str,
) -> Result<Vec<Diferencia>, Error> {
    let (de, a) = (objeto(de)?, objeto(a)?);
    let salida = git.ejecutar(
        raiz,
        [
            "diff-tree",
            "-r",
            "-z",
            "--no-renames",
            de.as_str(),
            a.as_str(),
        ],
    )?;
    // Pares de campos: ":<modo> <modo> <objeto> <objeto> <estado>\0<ruta>\0".
    let campos: Vec<&[u8]> = salida
        .split(|&b| b == 0)
        .filter(|c| !c.is_empty())
        .collect();
    let mut diferencias = Vec::new();
    for par in campos.chunks(2) {
        let [cabecera, ruta] = par else {
            return Err(inesperada(par[0]));
        };
        let texto = String::from_utf8_lossy(cabecera);
        let partes: Vec<&str> = texto.trim_start_matches(':').split(' ').collect();
        let [modo_antes, modo_despues, objeto_antes, objeto_despues, estado] = partes[..] else {
            return Err(inesperada(cabecera));
        };
        diferencias.push(Diferencia {
            modo_antes: modo_antes.to_string(),
            modo_despues: modo_despues.to_string(),
            objeto_antes: objeto_antes.to_string(),
            objeto_despues: objeto_despues.to_string(),
            estado: estado.to_string(),
            ruta: ruta.to_vec(),
        });
    }
    Ok(diferencias)
}

fn inesperada(registro: &[u8]) -> Error {
    Error::RespuestaInesperada(String::from_utf8_lossy(registro).into_owned())
}
