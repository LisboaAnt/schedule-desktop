/** Dados de demonstração — substituídos por Google Calendar / SQLite na Fase 2 */

export function padIso(y, m, d) {
  const mm = String(m + 1).padStart(2, "0");
  const dd = String(d).padStart(2, "0");
  return `${y}-${mm}-${dd}`;
}

/** @typedef {{ id: string, title: string, time?: string, color: string, calendarId?: string, startAt?: string|null, endAt?: string|null }} AgendaTask */

/** @type {Record<string, AgendaTask[]>} */
const DEMO = {};

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

/** @type {Record<string, AgendaTask[]>} */
let remoteByIso = {};

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
  return DEMO[iso] ? [...DEMO[iso]] : [];
}

export function taskCountForDay(iso) {
  if (useGoogleCalendar) {
    return remoteByIso[iso]?.length ?? 0;
  }
  return DEMO[iso]?.length ?? 0;
}
