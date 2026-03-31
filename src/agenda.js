/** Dados de demonstração — substituídos por Google Calendar / SQLite na Fase 2 */

export function padIso(y, m, d) {
  const mm = String(m + 1).padStart(2, "0");
  const dd = String(d).padStart(2, "0");
  return `${y}-${mm}-${dd}`;
}

/** @typedef {{ id: string, title: string, time?: string, color: string, calendarId?: string, startAt?: string|null, endAt?: string|null }} AgendaTask */

/** @type {Record<string, AgendaTask[]>} */
const DEMO = {};
const LOCAL_STORAGE_KEY = "agenda_local_events_v1";

function seedAroundToday() {
  const t = new Date();
  const y = t.getFullYear();
  const m = t.getMonth();
  const d = t.getDate();

  const add = (dy, tasks) => {
    const dt = new Date(y, m, d + dy);
    const iso = padIso(dt.getFullYear(), dt.getMonth(), dt.getDate());
    DEMO[iso] = tasks;
  };

  add(0, [
    { id: "a", title: "Stand-up", time: "09:30", color: "#5b8def" },
    { id: "b", title: "Revisão design", time: "14:00", color: "#a78bfa" },
    { id: "c", title: "Email cliente", color: "#94a3b8" },
  ]);
  add(1, [
    { id: "d", title: "Entrega relatório", time: "11:00", color: "#f59e0b" },
    { id: "e", title: "Ginásio", time: "18:30", color: "#34d399" },
  ]);
  add(2, [
    { id: "f", title: "1:1 com manager", time: "10:00", color: "#ec4899" },
  ]);
  add(3, [
    { id: "g", title: "Planeamento sprint", time: "15:00", color: "#5b8def" },
    { id: "h", title: "Compras", color: "#94a3b8" },
  ]);
  add(-1, [
    { id: "i", title: "Fechar PR", time: "16:00", color: "#22d3ee" },
  ]);
  add(-2, [
    { id: "j", title: "Dentista", time: "11:30", color: "#fb7185" },
  ]);

  const first = new Date(y, m, 1);
  const mid = new Date(y, m, 15);
  DEMO[padIso(first.getFullYear(), first.getMonth(), first.getDate())] = [
    { id: "k", title: "Renovar licença", color: "#fbbf24" },
  ];
  DEMO[padIso(mid.getFullYear(), mid.getMonth(), mid.getDate())] = [
    { id: "l", title: "Backup mensal", time: "08:00", color: "#64748b" },
    { id: "m", title: "Newsletter", time: "12:00", color: "#818cf8" },
  ];
}

seedAroundToday();

function cloneAgendaMap(map) {
  const out = {};
  for (const [iso, list] of Object.entries(map || {})) {
    out[iso] = Array.isArray(list) ? list.map((t) => ({ ...t })) : [];
  }
  return out;
}

function loadLocalMap() {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY);
    if (!raw) return cloneAgendaMap(DEMO);
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return cloneAgendaMap(DEMO);
    return cloneAgendaMap(parsed);
  } catch (_) {
    return cloneAgendaMap(DEMO);
  }
}

function persistLocalMap() {
  try {
    localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(localByIso));
  } catch (_) {
    /* ignorar quota/privacidade */
  }
}

/** @type {Record<string, AgendaTask[]>} */
let remoteByIso = {};
/** @type {Record<string, AgendaTask[]>} */
let localByIso = loadLocalMap();

/** Quando `true`, a grelha usa só eventos remotos (cache Google), não o DEMO. */
let useGoogleCalendar = false;

export function setRemoteTasksByIso(map) {
  remoteByIso = map && typeof map === "object" ? map : {};
}

export function setUseGoogleCalendar(connected) {
  useGoogleCalendar = Boolean(connected);
}

/** Vista com dados Google (eventos editáveis na API). */
export function isGoogleCalendarActive() {
  return useGoogleCalendar;
}

/**
 * @param {string} iso
 * @returns {AgendaTask[]}
 */
export function tasksForDay(iso) {
  if (useGoogleCalendar) {
    return remoteByIso[iso] ? [...remoteByIso[iso]] : [];
  }
  return localByIso[iso] ? [...localByIso[iso]] : [];
}

export function taskCountForDay(iso) {
  if (useGoogleCalendar) {
    return remoteByIso[iso]?.length ?? 0;
  }
  return localByIso[iso]?.length ?? 0;
}

function removeLocalById(id) {
  let touched = false;
  for (const iso of Object.keys(localByIso)) {
    const next = (localByIso[iso] || []).filter((t) => t.id !== id);
    if (next.length !== (localByIso[iso] || []).length) {
      touched = true;
      if (next.length > 0) localByIso[iso] = next;
      else delete localByIso[iso];
    }
  }
  return touched;
}

function timeFromIsoOrUndefined(startIso, allDay) {
  if (allDay || !startIso || !startIso.includes("T")) return undefined;
  const d = new Date(startIso);
  if (Number.isNaN(d.getTime())) return undefined;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

function dayIsoFromStart(startIso) {
  if (!startIso) return null;
  if (startIso.length === 10) return startIso;
  const d = new Date(startIso);
  if (Number.isNaN(d.getTime())) return null;
  return padIso(d.getFullYear(), d.getMonth(), d.getDate());
}

/**
 * @param {{ id?: string|null, summary: string, allDay: boolean, startIso: string, endIso: string, description?: string|null, location?: string|null }} payload
 * @returns {{ id: string }}
 */
export function upsertLocalTask(payload) {
  const id =
    typeof payload.id === "string" && payload.id.trim()
      ? payload.id.trim()
      : `local_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  const iso = dayIsoFromStart(payload.startIso);
  if (!iso) throw new Error("Evento local inválido: data inicial em falta.");
  removeLocalById(id);
  const item = {
    id,
    title: (payload.summary || "").trim() || "(sem título)",
    time: timeFromIsoOrUndefined(payload.startIso, payload.allDay),
    color: "#5b8def",
    startAt: payload.startIso,
    endAt: payload.endIso,
    description: payload.description || null,
    location: payload.location || null,
  };
  if (!localByIso[iso]) localByIso[iso] = [];
  localByIso[iso].push(item);
  persistLocalMap();
  return { id };
}

export function deleteLocalTask(id) {
  if (!id) return false;
  const removed = removeLocalById(id);
  if (removed) persistLocalMap();
  return removed;
}
