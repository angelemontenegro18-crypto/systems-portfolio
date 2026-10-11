// ops-console: la interfaz.
//
// Todo dato del servidor entra al documento como texto (textContent o nodos de texto) y los
// elementos se crean con createElement: nada de lo que llega se interpreta como marcado.
// Sin bibliotecas ni recursos externos.

const MARCA = "[REDACTADO]";
const numero = new Intl.NumberFormat("es");

const $ = (id) => document.getElementById(id);

function el(etiqueta, clase, texto) {
  const nodo = document.createElement(etiqueta);
  if (clase) nodo.className = clase;
  if (texto !== undefined && texto !== null) nodo.textContent = String(texto);
  return nodo;
}

function capitalizar(texto) {
  return texto ? texto.charAt(0).toUpperCase() + texto.slice(1) : texto;
}

function duracion(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
  return `${Math.floor(h / 24)} d`;
}

function bytes(n) {
  if (n < 1024) return `${numero.format(n)} B`;
  if (n < 1024 * 1024) return `${numero.format(Math.round(n / 1024))} KiB`;
  return `${numero.format(Math.round((n / 1024 / 1024) * 10) / 10)} MiB`;
}

function plural(n, singular, varios) {
  return `${n} ${n === 1 ? singular : varios}`;
}

// Una celda con un nombre largo (una unidad, una variable) que, en una pantalla angosta, se
// parte después de «_», «-» o «.» y no a mitad de palabra.
function celdaPartible(clase, texto) {
  const celda = el("td", clase);
  let trozo = "";
  for (const c of texto) {
    trozo += c;
    if (c === "_" || c === "-" || c === ".") {
      celda.append(document.createTextNode(trozo), document.createElement("wbr"));
      trozo = "";
    }
  }
  if (trozo) celda.append(document.createTextNode(trozo));
  return celda;
}

// --- La API -----------------------------------------------------------------------------

class ErrorApi extends Error {
  constructor(mensaje, codigo, estado) {
    super(mensaje);
    this.codigo = codigo;
    this.estado = estado;
  }
}

async function pedir(ruta, cuerpo) {
  const opciones = {
    cache: "no-store",
    credentials: "same-origin",
    headers: { Accept: "application/json" },
  };
  if (cuerpo !== undefined) {
    opciones.method = "POST";
    opciones.headers["Content-Type"] = "application/json";
    opciones.headers["X-Consola"] = "1";
    opciones.body = JSON.stringify(cuerpo);
  }
  let respuesta;
  try {
    respuesta = await fetch(ruta, opciones);
  } catch (_) {
    throw new ErrorApi("no hay conexión con la consola", "sin_conexion", 0);
  }
  let datos;
  try {
    datos = await respuesta.json();
  } catch (_) {
    throw new ErrorApi(`la respuesta no se entiende (HTTP ${respuesta.status})`, "ilegible", respuesta.status);
  }
  if (!datos || datos.ok !== true) {
    const e = (datos && datos.error) || {};
    throw new ErrorApi(e.mensaje || `error HTTP ${respuesta.status}`, e.codigo || "desconocido", respuesta.status);
  }
  return datos.datos;
}

// --- Avisos, conexión y estados de cada panel --------------------------------------------

function avisar(texto, tipo = "bien") {
  const nodo = el("div", "aviso", texto);
  nodo.dataset.tipo = tipo;
  $("avisos").append(nodo);
  setTimeout(() => nodo.remove(), tipo === "error" ? 9000 : 5000);
}

const conexion = { ultimoBien: 0, fallando: false };

function marcarConexion(bien) {
  const antes = conexion.fallando;
  if (bien) {
    conexion.ultimoBien = Date.now();
    conexion.fallando = false;
    if (antes) avisar("Conexión recuperada.");
  } else {
    conexion.fallando = true;
    if (!antes && conexion.ultimoBien) avisar("Se perdió la conexión con la consola; reintentando.", "error");
  }
  pintarConexion();
}

function pintarConexion() {
  const nodo = $("conexion");
  const texto = $("conexion-texto");
  if (conexion.fallando) {
    nodo.dataset.estado = "mal";
    texto.textContent = "Sin conexión · reintentando";
  } else if (conexion.ultimoBien) {
    nodo.dataset.estado = "bien";
    texto.textContent = `En línea · actualizado hace ${duracion(Date.now() - conexion.ultimoBien)}`;
  }
}

function mostrarEstado(id, texto, tipo) {
  const nodo = $(id);
  if (!texto) {
    nodo.hidden = true;
    nodo.textContent = "";
    delete nodo.dataset.tipo;
    return;
  }
  nodo.hidden = false;
  nodo.textContent = texto;
  if (tipo) nodo.dataset.tipo = tipo;
  else delete nodo.dataset.tipo;
}

function marcarInvalido(entrada, invalido) {
  if (invalido) entrada.setAttribute("aria-invalid", "true");
  else entrada.removeAttribute("aria-invalid");
}

// Una acción del usuario: el botón queda inactivo mientras dura, el error se avisa, y al
// final se refresca el grupo.
async function accion(boton, trabajo) {
  if (boton) boton.disabled = true;
  try {
    await trabajo();
  } catch (error) {
    avisar(`${capitalizar(error.message || String(error))}.`, "error");
  } finally {
    if (boton) boton.disabled = false;
    await refrescar("estado");
  }
}

// --- Grupo --------------------------------------------------------------------------------

const ESTADOS = {
  viva: { texto: "Viva", tono: "bien" },
  reciclada: { texto: "Reciclada", tono: "info" },
  muerta: { texto: "Muerta", tono: "mal" },
};

const MOTIVOS = {
  manual: "reciclada a mano",
  murio: "su proceso murió",
  vida_cumplida: "cumplió su tiempo de vida",
};

const tarjetas = new Map();
let ultimaRevision = null;

function crearTarjeta(id) {
  const li = el("li", "tarjeta");
  const cabeza = el("div", "tarjeta-cabeza");
  const insignia = el("span", "insignia");
  cabeza.append(el("h3", "tarjeta-titulo", `Posición ${id}`), insignia);

  const lista = el("dl");
  const campos = {};
  for (const [clave, etiqueta] of [
    ["pid", "Proceso"],
    ["desde", "Estado desde"],
    ["edad", "Edad"],
    ["generacion", "Generación"],
  ]) {
    const valor = el("dd");
    lista.append(el("dt", null, etiqueta), valor);
    campos[clave] = valor;
  }

  const boton = el("button", "boton", "Reciclar");
  boton.type = "button";
  boton.setAttribute("aria-label", `Reciclar la posición ${id}`);
  boton.addEventListener("click", () => reciclar(id, boton));

  li.append(cabeza, lista, boton);
  return { li, insignia, campos };
}

function pintarTarjetas(posiciones) {
  const lista = $("tarjetas");
  const vigentes = new Set(posiciones.map((p) => p.id));
  for (const [id, tarjeta] of tarjetas) {
    if (!vigentes.has(id)) {
      tarjeta.li.remove();
      tarjetas.delete(id);
    }
  }
  posiciones.forEach((p, indice) => {
    let tarjeta = tarjetas.get(p.id);
    if (!tarjeta) {
      tarjeta = crearTarjeta(p.id);
      tarjetas.set(p.id, tarjeta);
    }
    const estado = ESTADOS[p.estado] || { texto: p.estado, tono: "neutro" };
    tarjeta.li.dataset.estado = p.estado;
    tarjeta.insignia.dataset.tono = estado.tono;
    tarjeta.insignia.textContent = estado.texto;
    tarjeta.campos.pid.textContent = `pid ${p.pid}`;
    tarjeta.campos.desde.textContent = `hace ${duracion(p.hace_ms)}`;
    tarjeta.campos.edad.textContent = duracion(p.edad_ms);
    tarjeta.campos.generacion.textContent = String(p.generacion);
    // Solo se mueve lo que está fuera de lugar: la tarjeta (y el foco en su botón) se queda.
    if (lista.children[indice] !== tarjeta.li) lista.insertBefore(tarjeta.li, lista.children[indice] || null);
  });
}

function pintarRevision(ultimo) {
  const caja = $("revision");
  if (!ultimo) {
    caja.hidden = true;
    return;
  }
  caja.hidden = false;
  const origen = ultimo.origen === "manual" ? "pedida a mano" : "automática";
  const cambios = ultimo.acciones.length ? `${plural(ultimo.acciones.length, "reemplazo", "reemplazos")}.` : "sin cambios.";
  $("revision-resumen").textContent = `Hace ${duracion(ultimo.hace_ms)} (${origen}): ${cambios}`;
  const firma = JSON.stringify(ultimo.acciones);
  if (firma === ultimaRevision) return;
  ultimaRevision = firma;
  const lista = $("revision-acciones");
  lista.replaceChildren(
    ...ultimo.acciones.map((a) => {
      const motivo = MOTIVOS[a.motivo] || a.motivo;
      return el(
        "li",
        null,
        a.error
          ? `Posición ${a.posicion} (${motivo}): no se pudo reemplazar — ${a.error}`
          : `Posición ${a.posicion} (${motivo}): pid ${a.pid_anterior} → ${a.pid_nuevo}`,
      );
    }),
  );
}

// Los campos que el usuario está editando no se pisan con lo que llega del servidor.
const editados = new Set();
for (const id of ["cantidad", "minimo", "maximo"]) {
  $(id).addEventListener("input", () => editados.add(id));
}

function sincronizar(id, valor) {
  const entrada = $(id);
  if (editados.has(id) || document.activeElement === entrada) return;
  entrada.value = String(valor);
}

function pintarEstado(datos) {
  const { consola, politica, posiciones } = datos;
  const cuenta = (estado) => posiciones.filter((p) => p.estado === estado).length;
  $("grupo-resumen").textContent = [
    plural(posiciones.length, "posición", "posiciones"),
    `política ${politica.min}–${politica.max}`,
    `vida ${duracion(politica.vida_s * 1000)}`,
    [
      plural(cuenta("viva"), "viva", "vivas"),
      plural(cuenta("reciclada"), "reciclada", "recicladas"),
      plural(cuenta("muerta"), "muerta", "muertas"),
    ].join(", "),
  ].join(" · ");
  mostrarEstado("grupo-estado", posiciones.length ? null : "El grupo no tiene posiciones.");
  pintarTarjetas(posiciones);
  pintarRevision(datos.ultimo_tick);

  const cantidad = $("cantidad");
  cantidad.min = String(politica.min);
  cantidad.max = String(politica.max);
  $("ayuda-cantidad").textContent = `Entre ${politica.min} y ${politica.max}, según la política.`;
  for (const id of ["minimo", "maximo"]) $(id).max = String(politica.tope);
  sincronizar("cantidad", posiciones.length);
  sincronizar("minimo", politica.min);
  sincronizar("maximo", politica.max);

  $("dato-version").textContent = consola.version;
  $("dato-plataforma").textContent = consola.plataforma;
  $("dato-activa").textContent = duracion(consola.activa_ms);
}

function reciclar(id, boton) {
  const texto = boton.textContent;
  boton.textContent = "Reciclando…";
  accion(boton, async () => {
    const r = await pedir("/api/grupo/reciclar", { posicion: id });
    avisar(`Posición ${r.posicion} reciclada: pid ${r.pid_anterior} → ${r.pid_nuevo}.`);
  }).finally(() => {
    boton.textContent = texto;
  });
}

function leerEntero(id) {
  const entrada = $(id);
  const valor = Number.parseInt(entrada.value, 10);
  const valido = Number.isInteger(valor) && String(valor) === entrada.value.trim();
  marcarInvalido(entrada, !valido);
  return valido ? valor : null;
}

$("form-escala").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const cantidad = leerEntero("cantidad");
  if (cantidad === null) {
    avisar("Escribe la cantidad de posiciones como un número entero.", "error");
    return;
  }
  accion(evento.target.querySelector("button[type=submit]"), async () => {
    const r = await pedir("/api/grupo/escala", { cantidad });
    editados.delete("cantidad");
    avisar(r.antes === r.despues ? `El grupo ya tenía ${plural(r.despues, "posición", "posiciones")}.` : `Grupo: de ${r.antes} a ${plural(r.despues, "posición", "posiciones")}.`);
  });
});

$("form-politica").addEventListener("submit", (evento) => {
  evento.preventDefault();
  const min = leerEntero("minimo");
  const max = leerEntero("maximo");
  if (min === null || max === null) {
    avisar("Escribe el mínimo y el máximo como números enteros.", "error");
    return;
  }
  accion(evento.target.querySelector("button[type=submit]"), async () => {
    const r = await pedir("/api/grupo/escala", { min, max });
    for (const id of ["minimo", "maximo", "cantidad"]) editados.delete(id);
    const ajuste = r.antes === r.despues ? "." : `; el grupo pasó de ${r.antes} a ${r.despues}.`;
    avisar(`Política: entre ${r.politica.min} y ${r.politica.max} posiciones${ajuste}`);
  });
});

$("boton-tick").addEventListener("click", (evento) => {
  accion(evento.currentTarget, async () => {
    const r = await pedir("/api/grupo/tick", {});
    avisar(r.acciones.length ? `Revisión: ${plural(r.acciones.length, "reemplazo", "reemplazos")}.` : "Revisión: todo en orden, sin cambios.");
  });
});

// --- Servicios ----------------------------------------------------------------------------

const SERVICIO = {
  activo: { texto: "Activo", tono: "bien" },
  inactivo: { texto: "Inactivo", tono: "neutro" },
  fallido: { texto: "Fallido", tono: "mal" },
  en_transicion: { texto: "En transición", tono: "info" },
  no_encontrado: { texto: "No encontrado", tono: "aviso" },
  no_disponible: { texto: "No disponible", tono: "aviso" },
  tiempo_agotado: { texto: "Sin respuesta", tono: "mal" },
  error: { texto: "Error", tono: "mal" },
};

function pintarServicios(datos) {
  $("servicios-nota").textContent = `Estado según systemd. Cada consulta tiene ${datos.limite_s} s de límite y se recuerda ${datos.cache_s} s.`;
  const tabla = $("tabla-servicios");
  if (!datos.servicios.length) {
    tabla.hidden = true;
    mostrarEstado("servicios-estado", "No hay servicios vigilados: se agregan al arrancar, con --servicio UNIDAD.");
    return;
  }
  mostrarEstado("servicios-estado", null);
  tabla.hidden = false;
  $("servicios-filas").replaceChildren(
    ...datos.servicios.map((s) => {
      const forma = SERVICIO[s.estado] || { texto: s.estado, tono: "neutro" };
      const insignia = el("span", "insignia", forma.texto);
      insignia.dataset.tono = forma.tono;
      const estado = el("td");
      estado.append(insignia);
      if (s.detalle) estado.append(el("span", "detalle", s.detalle));
      estado.append(el("span", "detalle", s.de_cache ? `consultado hace ${duracion(s.hace_ms)}` : "recién consultado"));
      const fila = el("tr");
      fila.append(celdaPartible("fija", s.unidad), estado);
      return fila;
    }),
  );
}

// --- Sistema ------------------------------------------------------------------------------

function pintarSistema(datos) {
  const entradas = Object.entries(datos.variables);
  mostrarEstado("sistema-estado", entradas.length ? null : "No hay variables que vigilar.");
  $("tabla-variables").hidden = !entradas.length;
  $("variables-filas").replaceChildren(
    ...entradas.map(([nombre, definida]) => {
      const insignia = el("span", "insignia", definida ? "Definida" : "No definida");
      insignia.dataset.tono = definida ? "bien" : "neutro";
      const estado = el("td");
      estado.append(insignia);
      const fila = el("tr");
      fila.append(celdaPartible("fija", nombre), estado);
      return fila;
    }),
  );
}

// --- Registros ----------------------------------------------------------------------------

let pausado = false;
let ultimasLineas = null;

function lineaConMarcas(texto) {
  const li = el("li");
  if (/\bERROR\b/.test(texto)) li.dataset.nivel = "error";
  else if (/\bWARN(ING)?\b/.test(texto)) li.dataset.nivel = "aviso";
  texto.split(MARCA).forEach((parte, i) => {
    if (i > 0) {
      const marca = el("mark", "redactado", MARCA);
      marca.title = "Dato sensible tapado por la consola";
      li.append(marca);
    }
    if (parte) li.append(document.createTextNode(parte));
  });
  return li;
}

function pintarRegistros(datos) {
  const partes = [`${datos.archivo} · ${bytes(datos.bytes_archivo)}`];
  if (datos.recortado) partes.push(`se miran sus últimos ${bytes(datos.tope)}`);
  partes.push(`${plural(datos.redacciones, "dato tapado", "datos tapados")} en pantalla`);
  if (pausado) partes.push("en pausa");
  $("registros-nota").textContent = partes.join(" · ");

  const lista = $("registro-lineas");
  if (!datos.lineas.length) {
    lista.hidden = true;
    ultimasLineas = null;
    mostrarEstado("registros-estado", "El registro está vacío.");
    return;
  }
  mostrarEstado("registros-estado", null);
  const firma = datos.lineas.join("\n");
  if (firma === ultimasLineas && !lista.hidden) return;
  ultimasLineas = firma;
  // Si se estaba mirando el final, se sigue mirando el final; si no, no se mueve.
  const alFinal = lista.hidden || lista.scrollHeight - lista.scrollTop - lista.clientHeight < 24;
  lista.hidden = false;
  lista.replaceChildren(...datos.lineas.map(lineaConMarcas));
  if (alFinal) lista.scrollTop = lista.scrollHeight;
}

$("boton-pausa").addEventListener("click", (evento) => {
  const boton = evento.currentTarget;
  pausado = !pausado;
  boton.setAttribute("aria-pressed", String(pausado));
  boton.textContent = pausado ? "Reanudar" : "Pausar";
  if (!pausado) refrescar("registros");
});

$("lineas").addEventListener("change", () => {
  ultimasLineas = null;
  refrescar("registros");
});

// --- Actualización periódica --------------------------------------------------------------

const tareas = {
  estado: { cada: 2000, panel: "grupo-estado", ejecutar: () => pedir("/api/estado").then(pintarEstado) },
  servicios: { cada: 5000, panel: "servicios-estado", ejecutar: () => pedir("/api/servicios").then(pintarServicios) },
  registros: {
    cada: 3000,
    panel: "registros-estado",
    ejecutar: () => (pausado ? Promise.resolve() : pedir(`/api/registros?n=${encodeURIComponent($("lineas").value)}`).then(pintarRegistros)),
  },
  sistema: { cada: 15000, panel: "sistema-estado", ejecutar: () => pedir("/api/sistema").then(pintarSistema) },
};

function refrescar(nombre) {
  const tarea = tareas[nombre];
  if (tarea.enCurso) return tarea.enCurso;
  tarea.enCurso = tarea
    .ejecutar()
    .then(() => {
      tarea.pintado = true;
      marcarConexion(true);
    })
    .catch((error) => {
      if (error.codigo === "sin_conexion") marcarConexion(false);
      const viejo = tarea.pintado ? " Se muestran los últimos datos recibidos." : "";
      mostrarEstado(tarea.panel, `${capitalizar(error.message)}.${viejo}`, "error");
    })
    .finally(() => {
      tarea.enCurso = null;
    });
  return tarea.enCurso;
}

function programar(nombre) {
  const tarea = tareas[nombre];
  clearTimeout(tarea.temporizador);
  tarea.temporizador = setTimeout(async () => {
    if (!document.hidden) await refrescar(nombre);
    programar(nombre);
  }, tarea.cada);
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) for (const nombre of Object.keys(tareas)) refrescar(nombre);
});

setInterval(pintarConexion, 1000);

for (const nombre of Object.keys(tareas)) {
  refrescar(nombre);
  programar(nombre);
}
