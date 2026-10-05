//! Simular: calcular el plan sin escribir nada.
//!
//! Lee lo **commiteado** en la rama destino —no el árbol de trabajo—, porque es
//! sobre eso que después se prepara y se aplica. Toda la lectura pasa por
//! [`crate::lectura`]. El test `solo_lectura` comprueba sobre el código fuente
//! que este módulo no contiene ninguna forma de escribir.

use std::path::Path;

use crate::error::Error;
use crate::git::Git;
use crate::lectura;
use crate::plan::{Cambio, Omision, Omitido, Plan};
use crate::regla::Regla;
use crate::rutas::RutaRelativa;

/// Valida un nombre de rama con un subconjunto conservador de las reglas de
/// `git`: letras, dígitos, `.`, `_`, `-` y `/`; sin `..`, sin `//`, sin `-` ni
/// `/` al principio. Así ningún nombre puede pasar por una opción de `git`.
pub(crate) fn validar_rama(nombre: &str) -> Result<(), Error> {
    let valido = !nombre.is_empty()
        && nombre.len() <= 200
        && nombre
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        && !nombre.starts_with(['-', '/', '.'])
        && !nombre.ends_with(['/', '.'])
        && !nombre.ends_with(".lock")
        && !nombre.contains("..")
        && !nombre.contains("//")
        && !nombre.contains("/.");
    if valido {
        Ok(())
    } else {
        Err(Error::NombreDeRamaInvalido(nombre.to_string()))
    }
}

/// El plan de aplicar `reglas` a los archivos de texto de `destino`.
pub(crate) fn simular(
    git: &Git,
    raiz: &Path,
    destino: &str,
    reglas: &[Box<dyn Regla>],
) -> Result<Plan, Error> {
    validar_rama(destino)?;
    let base = lectura::commit_de_rama(git, raiz, destino)?
        .ok_or_else(|| Error::RamaInexistente(destino.to_string()))?;

    let mut cambios = Vec::new();
    let mut omitidos = Vec::new();
    for entrada in lectura::archivos(git, raiz, &base)? {
        let texto_ruta = String::from_utf8_lossy(&entrada.ruta).into_owned();
        let omitir = |motivo| Omitido {
            ruta: texto_ruta.clone(),
            motivo,
        };
        match (entrada.modo.as_str(), entrada.tipo.as_str()) {
            ("100644" | "100755", "blob") => {}
            ("120000", _) => {
                omitidos.push(omitir(Omision::EnlaceSimbolico));
                continue;
            }
            ("160000", _) => {
                omitidos.push(omitir(Omision::Submodulo));
                continue;
            }
            _ => {
                omitidos.push(omitir(Omision::Otro));
                continue;
            }
        }
        let Some(ruta) = std::str::from_utf8(&entrada.ruta)
            .ok()
            .and_then(|t| RutaRelativa::nueva(t).ok())
        else {
            omitidos.push(omitir(Omision::RutaInsegura));
            continue;
        };
        let antes = lectura::blob(git, raiz, &entrada.objeto)?;
        let texto = match std::str::from_utf8(&antes) {
            Ok(t) if !t.contains('\0') => t,
            _ => {
                omitidos.push(omitir(Omision::Binario));
                continue;
            }
        };

        let mut actual = texto.to_string();
        let mut aplicadas = Vec::new();
        for regla in reglas {
            let nuevo = regla.aplicar(&actual);
            if nuevo != actual {
                aplicadas.push(regla.nombre());
                actual = nuevo;
            }
        }
        if !aplicadas.is_empty() {
            cambios.push(Cambio {
                ruta,
                reglas: aplicadas,
                antes,
                despues: actual.into_bytes(),
            });
        }
    }
    Ok(Plan::nuevo(destino.to_string(), base, cambios, omitidos))
}
