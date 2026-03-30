import {
  padIso,
  tasksForDay,
  taskCountForDay,
  setRemoteTasksByIso,
  setUseGoogleCalendar,
} from "./agenda.js";

const { invoke } = window.__TAURI__.core;

const MONTH_NAMES = [
  "Janeiro",
  "Fevereiro",
  "Março",
  "Abril",
  "Maio",
  "Junho",
  "Julho",
  "Agosto",
  "Setembro",
  "Outubro",
  "Novembro",
  "Dezembro",
];

const WEEKDAYS = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
const WEEKDAYS_LONG = [
  "Segunda",
  "Terça",
  "Quarta",
  "Quinta",
  "Sexta",
  "Sábado",
  "Domingo",
];

/** @type {"month"|"week"|"day"} */
let agendaView = "month";

/** Data âncora para navegação (dia em foco) */
let cursor = new Date();

/** @type {{ viewMode: string, theme: string, widgetOpacity: number, agendaView?: string }} */
let appConfig = {
  viewMode: "widget",
  theme: "dark",
  widgetOpacity: 1,
  agendaView: "month",
};

function startOfWeek(d) {
  const x = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const wd = (x.getDay() + 6) % 7;
  x.setDate(x.getDate() - wd);
  return x;
}

function isToday(y, m, d) {
  const t = new Date();
  return t.getFullYear() === y && t.getMonth() === m && t.getDate() === d;
}

function sameDay(a, b) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** Mês/ano (e contexto do período) na barra superior. */
function setTitlePeriod(text) {
  const title = document.getElementById("title-period");
  if (title) title.textContent = text;
}

/** Recoloca a pílula após layout (janela mantém tamanho; calendário expande no espaço). */
function repositionRestorePill() {
  invoke("reposition_restore_pill").catch((e) => console.error(e));
}

window.__agendaRepositionRestorePill = repositionRestorePill;

function renderWeekdayHeader() {
  const row = document.getElementById("weekday-row");
  row.replaceChildren();
  for (const w of WEEKDAYS) {
    const el = document.createElement("div");
    el.textContent = w;
    row.append(el);
  }
}

function renderTaskDots(container, iso, max = 3) {
  const tasks = tasksForDay(iso);
  const slice = tasks.slice(0, max);
  for (const t of slice) {
    const dot = document.createElement("span");
    dot.className = "task-dot";
    dot.style.background = t.color;
    dot.title = t.title;
    container.append(dot);
  }
  if (tasks.length > max) {
    const more = document.createElement("span");
    more.className = "task-more";
    more.textContent = `+${tasks.length - max}`;
    container.append(more);
  }
}

function renderMonth() {
  const vy = cursor.getFullYear();
  const vm = cursor.getMonth();
  setTitlePeriod(`${MONTH_NAMES[vm]} ${vy}`);

  const grid = document.getElementById("day-grid");
  grid.replaceChildren();

  const first = new Date(vy, vm, 1);
  const startWeekday = (first.getDay() + 6) % 7;
  const daysInMonth = new Date(vy, vm + 1, 0).getDate();
  const prevMonthDays = new Date(vy, vm, 0).getDate();

  for (let i = 0; i < 42; i++) {
    const cell = document.createElement("div");
    cell.className = "cell";

    let y = vy;
    let m = vm;
    let d;

    if (i < startWeekday) {
      d = prevMonthDays - (startWeekday - 1 - i);
      m -= 1;
      if (m < 0) {
        m = 11;
        y -= 1;
      }
      cell.classList.add("other-month");
    } else if (i - startWeekday < daysInMonth) {
      d = i - startWeekday + 1;
      cell.classList.add("in-month");
    } else {
      const k = i - startWeekday - daysInMonth;
      const nx = new Date(vy, vm, daysInMonth + k + 1);
      y = nx.getFullYear();
      m = nx.getMonth();
      d = nx.getDate();
      cell.classList.add("other-month");
    }

    const iso = padIso(y, m, d);
    const num = document.createElement("span");
    num.className = "cell-day-num";
    num.textContent = String(d);
    cell.append(num);

    const tracks = document.createElement("div");
    tracks.className = "cell-tracks";
    if (taskCountForDay(iso) > 0) {
      renderTaskDots(tracks, iso, 4);
    }
    cell.append(tracks);

    cell.dataset.iso = iso;
    if (isToday(y, m, d)) cell.classList.add("today");
    if (sameDay(cursor, new Date(y, m, d)) && cell.classList.contains("in-month")) {
      cell.classList.add("selected");
    }

    if (cell.classList.contains("in-month")) {
      cell.addEventListener("click", async () => {
        cursor = new Date(y, m, d);
        agendaView = "day";
        appConfig.agendaView = "day";
        setActiveViewTab("day");
        renderAll();
        await persistConfig();
      });
    }

    grid.append(cell);
  }
}

function renderWeek() {
  const mon = startOfWeek(cursor);
  const sun = new Date(mon);
  sun.setDate(sun.getDate() + 6);

  setTitlePeriod(
    `${mon.getDate()} ${MONTH_NAMES[mon.getMonth()].slice(0, 3)} – ${sun.getDate()} ${MONTH_NAMES[sun.getMonth()]} ${sun.getFullYear()}`,
  );

  const wrap = document.getElementById("week-columns");
  wrap.replaceChildren();

  for (let i = 0; i < 7; i++) {
    const day = new Date(mon);
    day.setDate(day.getDate() + i);
    const iso = padIso(day.getFullYear(), day.getMonth(), day.getDate());
    const col = document.createElement("div");
    col.className = "week-col";
    if (isToday(day.getFullYear(), day.getMonth(), day.getDate())) {
      col.classList.add("is-today");
    }
    if (sameDay(day, cursor)) col.classList.add("is-selected");

    const head = document.createElement("div");
    head.className = "week-col-head";
    head.innerHTML = `<span class="w-dow">${WEEKDAYS[i]}</span><span class="w-dom">${day.getDate()}</span>`;
    head.addEventListener("click", async () => {
      cursor = day;
      agendaView = "day";
      appConfig.agendaView = "day";
      setActiveViewTab("day");
      renderAll();
      await persistConfig();
    });
    col.append(head);

    const list = document.createElement("div");
    list.className = "week-col-tasks";
    for (const t of tasksForDay(iso)) {
      list.append(renderTaskCard(t, false));
    }
    if (tasksForDay(iso).length === 0) {
      const empty = document.createElement("p");
      empty.className = "week-empty";
      empty.textContent = "Sem tarefas";
      list.append(empty);
    }
    col.append(list);
    wrap.append(col);
  }
}

function renderTaskCard(t, showTimeLine) {
  const el = document.createElement("div");
  el.className = "task-card";
  el.style.setProperty("--task-color", t.color);
  const time = t.time
    ? `<span class="task-time">${t.time}</span>`
    : showTimeLine
      ? `<span class="task-time muted">—</span>`
      : "";
  el.innerHTML = `${time}<span class="task-title">${escapeHtml(t.title)}</span>`;
  return el;
}

function escapeHtml(s) {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}

function renderDay() {
  const y = cursor.getFullYear();
  const m = cursor.getMonth();
  const d = cursor.getDate();
  const iso = padIso(y, m, d);
  const wd = (cursor.getDay() + 6) % 7;

  setTitlePeriod(`${WEEKDAYS_LONG[wd]}, ${d} de ${MONTH_NAMES[m]} ${y}`);

  const root = document.getElementById("day-detail");
  root.replaceChildren();

  const tasks = tasksForDay(iso);
  if (tasks.length === 0) {
    const p = document.createElement("p");
    p.className = "day-empty";
    p.textContent = "Nada agendado neste dia.";
    root.append(p);
    return;
  }

  tasks.sort((a, b) => (a.time || "").localeCompare(b.time || ""));
  for (const t of tasks) {
    root.append(renderTaskCard(t, true));
  }
}

function setActiveViewTab(view) {
  agendaView = view;
  document.querySelectorAll(".view-tabs .tab").forEach((btn) => {
    const v = btn.dataset.view;
    const on = v === view;
    btn.classList.toggle("active", on);
    btn.setAttribute("aria-selected", on ? "true" : "false");
  });

  document.getElementById("view-month").classList.toggle("hidden", view !== "month");
  document.getElementById("view-week").classList.toggle("hidden", view !== "week");
  document.getElementById("view-day").classList.toggle("hidden", view !== "day");
}

function renderAll() {
  renderWeekdayHeader();
  if (agendaView === "month") renderMonth();
  else if (agendaView === "week") renderWeek();
  else renderDay();
}

function navPrev() {
  if (agendaView === "month") {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth() - 1, 1);
  } else if (agendaView === "week") {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() - 7);
  } else {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() - 1);
  }
  renderAll();
}

function navNext() {
  if (agendaView === "month") {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth() + 1, 1);
  } else if (agendaView === "week") {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 7);
  } else {
    cursor = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 1);
  }
  renderAll();
}

function applyTheme(theme) {
  document.body.classList.remove("theme-dark", "theme-light");
  if (theme === "light") document.body.classList.add("theme-light");
  else if (theme === "dark") document.body.classList.add("theme-dark");
  else {
    const prefersDark =
      window.matchMedia &&
      window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.body.classList.add(prefersDark ? "theme-dark" : "theme-light");
  }
}

function applyViewMode(mode) {
  document.body.classList.remove("view-widget", "view-app");
  document.body.classList.add(mode === "app" ? "view-app" : "view-widget");
}

function applyOpacity(opacity) {
  const v = Math.min(1, Math.max(0.35, opacity));
  document.body.style.opacity = String(v);
}

async function persistConfig() {
  await invoke("save_app_config", {
    config: {
      viewMode: appConfig.viewMode,
      theme: appConfig.theme,
      widgetOpacity: appConfig.widgetOpacity,
      agendaView: appConfig.agendaView,
    },
  });
}

async function loadConfig() {
  try {
    const c = await invoke("get_app_config");
    appConfig = {
      viewMode: c.viewMode || "widget",
      theme: c.theme || "dark",
      widgetOpacity:
        typeof c.widgetOpacity === "number" ? c.widgetOpacity : 1,
      agendaView: c.agendaView || "month",
    };
  } catch (e) {
    console.warn("get_app_config", e);
  }
  applyTheme(appConfig.theme);
  applyViewMode(appConfig.viewMode);
  applyOpacity(appConfig.widgetOpacity);
  agendaView =
    appConfig.agendaView === "week" || appConfig.agendaView === "day"
      ? appConfig.agendaView
      : "month";
  setActiveViewTab(agendaView);

  document.getElementById("select-theme").value =
    appConfig.theme === "system"
      ? "system"
      : appConfig.theme === "light"
        ? "light"
        : "dark";
  document.getElementById("range-opacity").value = String(
    appConfig.widgetOpacity,
  );

  renderAll();
}

function syncSettingsToggleButton(inSettings) {
  const bSet = document.getElementById("btn-settings");
  if (inSettings) {
    bSet.textContent = "Agenda";
    bSet.title = "Voltar à agenda";
    bSet.setAttribute("aria-label", "Voltar à agenda");
    bSet.classList.add("btn-settings-as-agenda", "active");
  } else {
    bSet.textContent = "⚙";
    bSet.title = "Definições";
    bSet.setAttribute("aria-label", "Definições");
    bSet.classList.remove("btn-settings-as-agenda", "active");
  }
}

function updateSyncHint(state) {
  const h = document.getElementById("sync-hint");
  if (!h) return;
  if (state?.connected) {
    h.textContent =
      "Eventos do Google Calendar (cache local). Sincroniza em Definições se precisares de dados mais recentes.";
  } else {
    h.textContent =
      "Dados de demonstração na grelha. Liga o Google em Definições (client ID + OAuth).";
  }
}

/** @param {{ id: string, summary: string, startAt?: string|null, endAt?: string|null }} ev */
function taskFromCalendarEvent(ev) {
  const title = ev.summary || "(sem título)";
  let time;
  if (ev.startAt && ev.startAt.length > 10) {
    const d = new Date(ev.startAt);
    if (!Number.isNaN(d.getTime())) {
      time = d.toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
      });
    }
  }
  return { id: ev.id, title, time, color: "#7c9cf0" };
}

/** @param {{ startAt?: string|null }} ev */
function isoKeyFromCalendarEvent(ev) {
  if (!ev.startAt) return null;
  if (ev.startAt.length === 10) return ev.startAt;
  const d = new Date(ev.startAt);
  if (Number.isNaN(d.getTime())) return null;
  return padIso(d.getFullYear(), d.getMonth(), d.getDate());
}

/** @param {Array<{ id: string, summary: string, startAt?: string|null }>} events */
function eventsToByIso(events) {
  /** @type {Record<string, import('./agenda.js').AgendaTask[]>} */
  const map = {};
  for (const ev of events) {
    const iso = isoKeyFromCalendarEvent(ev);
    if (!iso) continue;
    if (!map[iso]) map[iso] = [];
    map[iso].push(taskFromCalendarEvent(ev));
  }
  return map;
}

async function applyGoogleCalendarToGrid() {
  try {
    const s = await invoke("get_calendar_state");
    setUseGoogleCalendar(s.connected);
    updateSyncHint(s);
    if (s.connected) {
      const events = await invoke("get_cached_calendar_events");
      setRemoteTasksByIso(eventsToByIso(events));
    } else {
      setRemoteTasksByIso({});
    }
    renderAll();
  } catch (e) {
    console.warn("applyGoogleCalendarToGrid", e);
  }
}

function setCalendarOAuthMessage(text) {
  const m = document.getElementById("calendar-oauth-msg");
  if (m) m.textContent = text || "";
}

function updateGoogleButtons(state) {
  const signIn = document.getElementById("btn-google-sign-in");
  const sync = document.getElementById("btn-google-sync");
  const disc = document.getElementById("btn-google-disconnect");
  const hint = document.getElementById("calendar-client-id-hint");
  if (!signIn || !sync || !disc) return;

  const cfg = Boolean(state.clientIdConfigured);
  if (hint) {
    if (!cfg) {
      hint.classList.remove("hidden");
      hint.textContent =
        "Para OAuth: define a variável GOOGLE_OAUTH_CLIENT_ID ou cria o ficheiro google_oauth_client_id.txt na pasta de configuração da app (ver docs/GOOGLE-CALENDAR-FASE2.md). Na Google Cloud, regista o redirect http://127.0.0.1:PORT/callback (porta dinâmica).";
    } else {
      hint.classList.add("hidden");
      hint.textContent = "";
    }
  }

  signIn.disabled = !cfg || state.connected;
  sync.disabled = !cfg || !state.connected;
  disc.disabled = !cfg || !state.connected;
}

async function refreshCalendarStateLine() {
  const el = document.getElementById("calendar-state-line");
  if (!el) return;
  try {
    const s = await invoke("get_calendar_state");
    updateSyncHint(s);
    updateGoogleButtons(s);
    const db = s.dbReady ? "Base local (cache) pronta." : "Base local indisponível.";
    if (!s.clientIdConfigured) {
      el.textContent = `${db} Client OAuth não configurado — a grelha usa dados de demonstração.`;
    } else if (!s.connected) {
      el.textContent = `${db} Client ID configurado. Inicia sessão com Google para substituir o demo na grelha.`;
    } else {
      el.textContent = `${db} Conta Google ligada — a grelha mostra eventos em cache (sincroniza para atualizar).`;
    }
  } catch (e) {
    el.textContent = `Não foi possível ler o estado: ${e?.message || String(e)}`;
  }
}

function showPanel(which) {
  const agenda = document.getElementById("agenda-panel");
  const set = document.getElementById("settings-panel");
  if (which === "settings") {
    agenda.classList.add("hidden");
    set.classList.remove("hidden");
    syncSettingsToggleButton(true);
    void refreshCalendarStateLine();
  } else {
    agenda.classList.remove("hidden");
    set.classList.add("hidden");
    syncSettingsToggleButton(false);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("[data-action='period-prev']").forEach((el) => {
    el.addEventListener("click", () => navPrev());
  });
  document.querySelectorAll("[data-action='period-next']").forEach((el) => {
    el.addEventListener("click", () => navNext());
  });

  document.querySelectorAll(".view-tabs .tab").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const v = /** @type {"month"|"week"|"day"} */ (btn.dataset.view);
      setActiveViewTab(v);
      appConfig.agendaView = v;
      renderAll();
      await persistConfig();
    });
  });

  document.getElementById("btn-send-back").addEventListener("click", async () => {
    try {
      await invoke("send_window_to_back");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-bring-front").addEventListener("click", async () => {
    try {
      await invoke("bring_window_to_front");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-mode").addEventListener("click", async () => {
    appConfig.viewMode = appConfig.viewMode === "app" ? "widget" : "app";
    applyViewMode(appConfig.viewMode);
    await persistConfig();
  });

  document.getElementById("btn-settings").addEventListener("click", () => {
    const set = document.getElementById("settings-panel");
    if (set.classList.contains("hidden")) {
      showPanel("settings");
    } else {
      showPanel("cal");
    }
  });

  document.getElementById("select-theme").addEventListener("change", async (e) => {
    appConfig.theme = e.target.value;
    applyTheme(appConfig.theme);
    await persistConfig();
  });

  document
    .getElementById("range-opacity")
    .addEventListener("input", (e) => {
      appConfig.widgetOpacity = Number(e.target.value);
      applyOpacity(appConfig.widgetOpacity);
    });

  document
    .getElementById("range-opacity")
    .addEventListener("change", async (e) => {
      appConfig.widgetOpacity = Number(e.target.value);
      await persistConfig();
    });

  if (window.matchMedia) {
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", () => {
        if (appConfig.theme === "system") applyTheme("system");
      });
  }

  loadConfig();
});
