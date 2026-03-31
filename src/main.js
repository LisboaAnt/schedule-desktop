import {
  padIso,
  tasksForDay,
  taskCountForDay,
  setRemoteTasksByIso,
  setUseGoogleCalendar,
  isGoogleCalendarActive,
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

/** @type {{ viewMode: string, theme: string, widgetOpacity: number, agendaView?: string, autoSyncMinutes?: number, closeToTray?: boolean }} */
let appConfig = {
  viewMode: "widget",
  theme: "dark",
  widgetOpacity: 1,
  agendaView: "month",
  autoSyncMinutes: 0,
  closeToTray: false,
};

/** @type {ReturnType<typeof setInterval> | null} */
let calendarAutoSyncTimer = null;
let lastWindowFocusSyncMs = 0;
/** @type {ReturnType<typeof setTimeout> | null} */
let monthLayoutDebounce = null;

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

/**
 * Linhas de atividade por célula no mês, a partir da altura da vista (6 linhas × 7 colunas).
 */
function getMonthMaxVisibleTasks() {
  const viewMonth = document.getElementById("view-month");
  if (!viewMonth || viewMonth.classList.contains("hidden")) return 3;
  const weekdayRow = document.getElementById("weekday-row");
  const headH = weekdayRow?.offsetHeight ?? 22;
  const gap = 8;
  const gridH = viewMonth.clientHeight - headH - gap;
  if (gridH < 36) return 2;
  const cellH = gridH / 6;
  const dayAndPadding = 24;
  const chipRow = 18;
  const usable = Math.max(0, cellH - dayAndPadding);
  const n = Math.floor(usable / chipRow);
  return Math.max(2, Math.min(18, n));
}

function scheduleMonthRelayoutFromResize() {
  if (agendaView !== "month") return;
  if (monthLayoutDebounce) clearTimeout(monthLayoutDebounce);
  monthLayoutDebounce = setTimeout(() => {
    monthLayoutDebounce = null;
    renderMonth();
  }, 120);
}

/** Vista mês: hora + título quando a célula tem largura; só pontos quando é estreita. */
function renderMonthCellTasks(container, iso, maxVisible) {
  const tasks = [...tasksForDay(iso)].sort((a, b) => {
    const aAll = !a.time;
    const bAll = !b.time;
    if (aAll !== bAll) return aAll ? -1 : 1;
    if (a.time && b.time) {
      const c = a.time.localeCompare(b.time);
      if (c !== 0) return c;
    }
    return (a.title || "").localeCompare(b.title || "", undefined, {
      sensitivity: "base",
    });
  });
  const slice = tasks.slice(0, maxVisible);
  for (const t of slice) {
    const chip = document.createElement("div");
    chip.className = "task-month-chip";
    chip.title = t.time ? `${t.time} · ${t.title}` : t.title;
    const dot = document.createElement("span");
    dot.className = "task-month-dot";
    dot.style.background = t.color;
    dot.setAttribute("aria-hidden", "true");
    const textWrap = document.createElement("span");
    textWrap.className = "task-month-text";
    if (t.time) {
      const timeEl = document.createElement("span");
      timeEl.className = "task-month-time";
      timeEl.textContent = t.time;
      textWrap.append(timeEl);
    }
    const titleEl = document.createElement("span");
    titleEl.className = "task-month-title";
    titleEl.textContent = t.title;
    textWrap.append(titleEl);
    chip.append(dot, textWrap);
    container.append(chip);
  }
  if (tasks.length > maxVisible) {
    const more = document.createElement("span");
    more.className = "task-more";
    more.textContent = `+${tasks.length - maxVisible}`;
    more.title = `${tasks.length - maxVisible} mais`;
    container.append(more);
  }
}

function renderMonth() {
  const vy = cursor.getFullYear();
  const vm = cursor.getMonth();
  setTitlePeriod(`${MONTH_NAMES[vm]} ${vy}`);

  const maxVisible = getMonthMaxVisibleTasks();
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
      renderMonthCellTasks(tracks, iso, maxVisible);
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

function hhmm(d) {
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h}:${m}`;
}

function syncGcAlldayUi() {
  const on = Boolean(document.getElementById("gc-allday")?.checked);
  document.getElementById("gc-timed-block")?.classList.toggle("is-hidden", on);
  document.getElementById("gc-allday-block")?.classList.toggle("is-visible", on);
}

function updateGcTimezoneLabel() {
  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone || "";
    const off = -new Date().getTimezoneOffset();
    const sign = off >= 0 ? "+" : "-";
    const h = Math.floor(Math.abs(off) / 60);
    const m = Math.abs(off) % 60;
    const gmt = `GMT${sign}${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
    const el = document.getElementById("gc-tz-label");
    if (el) el.textContent = tz ? `(${gmt}) ${tz}` : `(${gmt})`;
  } catch (_) {
    /* ignore */
  }
}

function clampGcEndDateToStart() {
  const s = document.getElementById("gc-start-date")?.value;
  const e = document.getElementById("gc-end-date");
  if (!s || !e?.value) return;
  if (e.value < s) e.value = s;
}

/**
 * Lê o formulário do editor estilo Google Calendar.
 * @returns {{ summary: string, allDay: boolean, startIso: string, endIso: string, description: string, location: string }}
 */
function readGoogleCalendarEditorForm() {
  const summary = document.getElementById("gc-title")?.value?.trim() || "";
  if (!summary) {
    throw new Error("Escreve um título para o evento.");
  }
  const allDay = Boolean(document.getElementById("gc-allday")?.checked);
  const description = (
    document.getElementById("gc-description")?.value ?? ""
  ).trim();
  const location = (document.getElementById("gc-location")?.value ?? "").trim();

  if (allDay) {
    const date = document.getElementById("gc-allday-date")?.value;
    if (!date) {
      throw new Error("Escolhe uma data.");
    }
    const d0 = new Date(`${date}T12:00:00`);
    const next = new Date(d0);
    next.setDate(next.getDate() + 1);
    const endStr = padIso(next.getFullYear(), next.getMonth(), next.getDate());
    return {
      summary,
      allDay: true,
      startIso: date,
      endIso: endStr,
      description,
      location,
    };
  }

  const dS = document.getElementById("gc-start-date")?.value;
  const dE = document.getElementById("gc-end-date")?.value;
  const tS = document.getElementById("gc-start-time")?.value || "09:00";
  const tE = document.getElementById("gc-end-time")?.value || "10:00";
  if (!dS || !dE) {
    throw new Error("Escolhe as datas de início e fim.");
  }
  const start = new Date(`${dS}T${tS}:00`);
  let end = new Date(`${dE}T${tE}:00`);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime())) {
    throw new Error("Data ou hora inválida.");
  }
  if (end <= start) {
    end = new Date(start.getTime() + 60 * 60 * 1000);
  }
  return {
    summary,
    allDay: false,
    startIso: start.toISOString(),
    endIso: end.toISOString(),
    description,
    location,
  };
}

/** Criar: vazio → null (omitir). Editar: vazio → "" (limpar no Google). */
function descriptionLocationForPayload(description, location, isEdit) {
  if (isEdit) {
    return { description, location };
  }
  return {
    description: description.length ? description : null,
    location: location.length ? location : null,
  };
}

/** @param {string[]|null|undefined} rules */
function recurrenceSelectFromRules(rules) {
  if (!rules?.length) return "none";
  const r = String(rules[0] || "");
  if (r.includes("FREQ=DAILY")) return "daily";
  if (r.includes("FREQ=WEEKLY")) return "weekly";
  if (r.includes("FREQ=MONTHLY")) return "monthly";
  if (r.includes("FREQ=YEARLY")) return "yearly";
  return "none";
}

function syncGcReminderUi() {
  const def = document.getElementById("gc-reminders-default")?.checked ?? true;
  const row = document.getElementById("gc-reminder-custom-row");
  const sel = document.getElementById("gc-reminder-minutes");
  if (row) row.classList.toggle("is-disabled", def);
  if (sel) sel.disabled = def;
}

/** @param {null | { form?: Record<string, unknown> | null }} t */
function syncGcMeetRow(t) {
  const link = document.getElementById("gc-meet-open");
  const label = document.getElementById("gc-meet-add-label");
  const cb = document.getElementById("gc-request-meet");
  const url = /** @type {string|undefined} */ (t?.form?.hangoutLink);
  if (link && label) {
    if (url) {
      link.href = url;
      link.classList.remove("hidden");
      label.classList.add("hidden");
      if (cb) cb.checked = false;
    } else {
      link.classList.add("hidden");
      link.href = "#";
      label.classList.remove("hidden");
    }
  }
}

function resetGcExtendedForm() {
  const rep = document.getElementById("gc-repeat");
  if (rep) rep.value = "none";
  const cb = document.getElementById("gc-request-meet");
  if (cb) cb.checked = false;
  const rd = document.getElementById("gc-reminders-default");
  if (rd) rd.checked = true;
  const rm = document.getElementById("gc-reminder-minutes");
  if (rm) rm.value = "30";
  const tr = document.getElementById("gc-transparency");
  if (tr) tr.value = "opaque";
  const vis = document.getElementById("gc-visibility");
  if (vis) vis.value = "default";
  const col = document.getElementById("gc-color");
  if (col) col.value = "";
  const g = document.getElementById("gc-guests");
  if (g) g.value = "";
  const gm = document.getElementById("gc-guest-modify");
  if (gm) gm.checked = false;
  const gi = document.getElementById("gc-guest-invite");
  if (gi) gi.checked = true;
  const gs = document.getElementById("gc-guest-see");
  if (gs) gs.checked = true;
  syncGcReminderUi();
  syncGcMeetRow(null);
}

/**
 * @param {{ form?: Record<string, unknown> | null }} t
 */
function fillGcExtendedFormFromTask(t) {
  resetGcExtendedForm();
  const f = t?.form;
  if (!f) {
    syncGcMeetRow(t);
    return;
  }
  const rep = document.getElementById("gc-repeat");
  if (rep && Array.isArray(f.recurrence) && f.recurrence.length) {
    rep.value = recurrenceSelectFromRules(
      /** @type {string[]} */ (f.recurrence),
    );
  }
  if (f.remindersUseDefault === false) {
    const rd = document.getElementById("gc-reminders-default");
    if (rd) rd.checked = false;
    if (typeof f.reminderPopupMinutes === "number") {
      const m = String(f.reminderPopupMinutes);
      const sel = document.getElementById("gc-reminder-minutes");
      if (sel && [...sel.options].some((o) => o.value === m)) sel.value = m;
    }
  } else if (f.remindersUseDefault === true) {
    const rd = document.getElementById("gc-reminders-default");
    if (rd) rd.checked = true;
  }
  if (typeof f.transparency === "string" && f.transparency) {
    const el = document.getElementById("gc-transparency");
    if (el) el.value = f.transparency;
  }
  if (typeof f.visibility === "string" && f.visibility) {
    const el = document.getElementById("gc-visibility");
    if (el) el.value = f.visibility;
  }
  if (typeof f.colorId === "string" && f.colorId) {
    const el = document.getElementById("gc-color");
    if (el && [...el.options].some((o) => o.value === f.colorId)) {
      el.value = f.colorId;
    }
  }
  if (Array.isArray(f.attendees) && f.attendees.length) {
    const emails = f.attendees
      .map((a) =>
        typeof a === "object" && a && "email" in a
          ? String(/** @type {{ email?: string }} */ (a).email || "")
          : "",
      )
      .filter(Boolean);
    const ta = document.getElementById("gc-guests");
    if (ta) ta.value = emails.join(", ");
  }
  if (typeof f.guestsCanModify === "boolean") {
    const el = document.getElementById("gc-guest-modify");
    if (el) el.checked = f.guestsCanModify;
  }
  if (typeof f.guestsCanInviteOthers === "boolean") {
    const el = document.getElementById("gc-guest-invite");
    if (el) el.checked = f.guestsCanInviteOthers;
  }
  if (typeof f.guestsCanSeeOtherGuests === "boolean") {
    const el = document.getElementById("gc-guest-see");
    if (el) el.checked = f.guestsCanSeeOtherGuests;
  }
  syncGcReminderUi();
  syncGcMeetRow(t);
}

function readEventExtensions() {
  const linkEl = document.getElementById("gc-meet-open");
  const hasMeetLink =
    linkEl &&
    !linkEl.classList.contains("hidden") &&
    linkEl.getAttribute("href") &&
    linkEl.getAttribute("href") !== "#";
  const addLabel = document.getElementById("gc-meet-add-label");
  const canRequestMeet = addLabel && !addLabel.classList.contains("hidden");
  const requestMeet =
    Boolean(canRequestMeet && document.getElementById("gc-request-meet")?.checked);
  const guestsRaw = document.getElementById("gc-guests")?.value || "";
  const attendees = guestsRaw
    .split(/[,;\n]+/)
    .map((e) => e.trim().toLowerCase())
    .filter((e) => e.includes("@") && e.includes(".") && e.length > 5);
  const colorRaw = document.getElementById("gc-color")?.value?.trim();
  return {
    requestGoogleMeet: requestMeet && !hasMeetLink,
    recurrence: document.getElementById("gc-repeat")?.value || "none",
    useDefaultReminders:
      document.getElementById("gc-reminders-default")?.checked ?? true,
    reminderMinutes: Math.min(
      40320,
      Number(document.getElementById("gc-reminder-minutes")?.value) || 30,
    ),
    transparency: document.getElementById("gc-transparency")?.value || "opaque",
    visibility: document.getElementById("gc-visibility")?.value || "default",
    colorId: colorRaw && colorRaw !== "" ? colorRaw : null,
    attendees,
    guestsCanModify: Boolean(document.getElementById("gc-guest-modify")?.checked),
    guestsCanInviteOthers:
      document.getElementById("gc-guest-invite")?.checked !== false,
    guestsCanSeeOtherGuests:
      document.getElementById("gc-guest-see")?.checked !== false,
  };
}

function closeGcMoreMenu() {
  const d = document.getElementById("gc-more-details");
  if (d) d.open = false;
}

function closeEventEditor() {
  const ov = document.getElementById("event-editor-overlay");
  if (!ov) return;
  ov.classList.add("hidden");
  ov.setAttribute("aria-hidden", "true");
  closeGcMoreMenu();
}

/**
 * @param {null | { id: string, title: string, calendarId?: string, startAt?: string|null, endAt?: string|null, description?: string|null, location?: string|null, form?: Record<string, unknown>|null }} t
 *   `null` = novo evento (data baseada em `cursor`).
 */
function openEventEditor(t) {
  const ov = document.getElementById("event-editor-overlay");
  if (!ov) return;
  if (t) {
    if (!isGoogleCalendarActive() || !t.calendarId) return;
  } else if (!isGoogleCalendarActive()) {
    return;
  }

  const idEl = document.getElementById("edit-event-id");
  const calEl = document.getElementById("edit-calendar-id");
  const more = document.getElementById("gc-more-details");

  if (!t) {
    if (idEl) idEl.value = "";
    if (calEl) calEl.value = "primary";
    document.getElementById("gc-title").value = "";
    document.getElementById("gc-description").value = "";
    document.getElementById("gc-location").value = "";
    document.getElementById("gc-allday").checked = false;
    const iso = padIso(
      cursor.getFullYear(),
      cursor.getMonth(),
      cursor.getDate(),
    );
    document.getElementById("gc-start-date").value = iso;
    document.getElementById("gc-end-date").value = iso;
    document.getElementById("gc-allday-date").value = iso;
    document.getElementById("gc-start-time").value = "09:00";
    document.getElementById("gc-end-time").value = "10:00";
    more?.classList.add("hidden");
    closeGcMoreMenu();
    resetGcExtendedForm();
  } else {
    if (idEl) idEl.value = t.id;
    if (calEl) calEl.value = t.calendarId || "primary";
    document.getElementById("gc-title").value = t.title || "";
    document.getElementById("gc-description").value = t.description || "";
    document.getElementById("gc-location").value = t.location || "";

    const allday = Boolean(t.startAt && t.startAt.length === 10);
    document.getElementById("gc-allday").checked = allday;

    if (allday && t.startAt) {
      document.getElementById("gc-allday-date").value = t.startAt.slice(0, 10);
    } else if (t.startAt) {
      const s = new Date(t.startAt);
      const e =
        t.endAt && t.endAt.length > 10
          ? new Date(t.endAt)
          : new Date(s.getTime() + 3600000);
      document.getElementById("gc-start-date").value = padIso(
        s.getFullYear(),
        s.getMonth(),
        s.getDate(),
      );
      document.getElementById("gc-end-date").value = padIso(
        e.getFullYear(),
        e.getMonth(),
        e.getDate(),
      );
      document.getElementById("gc-start-time").value = hhmm(s);
      document.getElementById("gc-end-time").value = hhmm(e);
    } else {
      const iso = padIso(
        cursor.getFullYear(),
        cursor.getMonth(),
        cursor.getDate(),
      );
      document.getElementById("gc-start-date").value = iso;
      document.getElementById("gc-end-date").value = iso;
    }

    more?.classList.remove("hidden");
    closeGcMoreMenu();
    fillGcExtendedFormFromTask(t);
  }

  syncGcAlldayUi();
  updateGcTimezoneLabel();
  ov.classList.remove("hidden");
  ov.setAttribute("aria-hidden", "false");
  document.getElementById("gc-title")?.focus();
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
  if (isGoogleCalendarActive() && t.calendarId) {
    el.classList.add("task-card--google");
    el.title = "Clicar para editar ou apagar";
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      openEventEditor(t);
    });
  }
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
  /** @type {Record<string, unknown>} */
  let prev = {};
  try {
    prev = await invoke("get_app_config");
  } catch (_) {
    /* primeira gravação */
  }
  await invoke("save_app_config", {
    config: {
      viewMode: appConfig.viewMode,
      theme: appConfig.theme,
      widgetOpacity: appConfig.widgetOpacity,
      agendaView: appConfig.agendaView,
      desktopBehindIcons: Boolean(prev.desktopBehindIcons),
      autoSyncMinutes:
        typeof appConfig.autoSyncMinutes === "number"
          ? appConfig.autoSyncMinutes
          : Number(prev.autoSyncMinutes) || 0,
      closeToTray:
        typeof appConfig.closeToTray === "boolean"
          ? appConfig.closeToTray
          : Boolean(prev.closeToTray),
    },
  });
}

async function loadConfig() {
  try {
    const c = await invoke("get_app_config");
    const allowed = new Set([0, 5, 15, 30, 60]);
    const rawMin = Number(c.autoSyncMinutes);
    const autoSyncMinutes = allowed.has(rawMin) ? rawMin : 0;
    appConfig = {
      viewMode: c.viewMode || "widget",
      theme: c.theme || "dark",
      widgetOpacity:
        typeof c.widgetOpacity === "number" ? c.widgetOpacity : 1,
      agendaView: c.agendaView || "month",
      autoSyncMinutes,
      closeToTray: Boolean(c.closeToTray),
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

  const autoSel = document.getElementById("select-auto-sync");
  if (autoSel) {
    autoSel.value = String(appConfig.autoSyncMinutes ?? 0);
  }

  const closeTray = document.getElementById("chk-close-to-tray");
  if (closeTray) closeTray.checked = Boolean(appConfig.closeToTray);

  const layoutSel = document.getElementById("select-view-layout");
  if (layoutSel) {
    layoutSel.value = appConfig.viewMode === "app" ? "app" : "widget";
  }

  void refreshAutostartCheckbox();

  renderAll();
  await applyGoogleCalendarToGrid();
  void syncMaximizeButton();
}

async function refreshAutostartCheckbox() {
  const cb = document.getElementById("chk-start-windows");
  if (!cb) return;
  try {
    const on = await invoke("autostart_is_enabled");
    cb.checked = Boolean(on);
    cb.disabled = false;
  } catch (e) {
    console.warn("autostart_is_enabled", e);
    cb.disabled = true;
    cb.title = "Indisponível nesta plataforma ou build.";
  }
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
    let t =
      "Google Calendar: usa + na barra para novo evento; na Semana ou Dia, clica num evento para editar. Podes ativar sincronização automática em Definições.";
    const n = state.pendingMutationsCount ?? 0;
    if (n > 0) {
      t += ` Há ${n} alteração(ões) na fila offline — sincroniza para tentar enviar ao Google.`;
    }
    h.textContent = t;
  } else {
    h.textContent =
      "Dados de demonstração na grelha. Liga o Google em Definições (client ID + OAuth).";
  }
}

/** @param {{ id: string, summary: string, calendarId?: string, startAt?: string|null, endAt?: string|null, description?: string|null, location?: string|null }} ev */
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
  return {
    id: ev.id,
    title,
    time,
    color: "#7c9cf0",
    calendarId: ev.calendarId || "primary",
    startAt: ev.startAt ?? null,
    endAt: ev.endAt ?? null,
    description: ev.description ?? null,
    location: ev.location ?? null,
    form: ev.form ?? null,
  };
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
  /** @type {Record<string, Array<{ id: string, title: string, time?: string, color: string }>>} */
  const map = {};
  for (const ev of events) {
    const iso = isoKeyFromCalendarEvent(ev);
    if (!iso) continue;
    if (!map[iso]) map[iso] = [];
    map[iso].push(taskFromCalendarEvent(ev));
  }
  return map;
}

/** @param {Record<string, unknown>} raw */
function normalizeCalendarState(raw) {
  const clientIdConfigured = Boolean(
    raw.clientIdConfigured ?? raw.client_id_configured,
  );
  const connected = Boolean(raw.connected);
  const dbReady = Boolean(raw.dbReady ?? raw.db_ready);
  const source = typeof raw.source === "string" ? raw.source : "demo";
  const pendingMutationsCount = Number(
    raw.pendingMutationsCount ?? raw.pending_mutations_count ?? 0,
  );
  return {
    source,
    connected,
    dbReady,
    clientIdConfigured,
    pendingMutationsCount: Number.isFinite(pendingMutationsCount)
      ? Math.max(0, Math.floor(pendingMutationsCount))
      : 0,
  };
}

/** Depois de OAuth com sucesso: o backend já tem refresh token — garante botões ativos mesmo se o estado IPC atrasar. */
function applyGoogleButtonsSignedIn() {
  const row = document.getElementById("calendar-actions");
  const signIn = document.getElementById("btn-google-sign-in");
  const sync = document.getElementById("btn-google-sync");
  const disc = document.getElementById("btn-google-disconnect");
  if (!signIn || !sync || !disc) return;
  row?.classList.add("has-google-session");
  signIn.disabled = true;
  sync.disabled = false;
  disc.disabled = false;
}

async function calendarSyncFromApiQuiet() {
  if (!isGoogleCalendarActive()) return;
  try {
    await invoke("google_calendar_sync");
    const events = await invoke("get_cached_calendar_events");
    setRemoteTasksByIso(eventsToByIso(events));
    renderAll();
    await bumpCalendarHintsFromBackend();
  } catch (e) {
    console.warn("calendarSyncFromApiQuiet", e);
  }
}

function restartCalendarAutoSync() {
  if (calendarAutoSyncTimer != null) {
    clearInterval(calendarAutoSyncTimer);
    calendarAutoSyncTimer = null;
  }
  const min = appConfig.autoSyncMinutes ?? 0;
  if (!min || !isGoogleCalendarActive()) return;
  calendarAutoSyncTimer = setInterval(() => {
    void calendarSyncFromApiQuiet();
  }, min * 60 * 1000);
}

async function syncCalendarOnWindowFocusThrottled() {
  if (!isGoogleCalendarActive()) return;
  const now = Date.now();
  if (now - lastWindowFocusSyncMs < 90_000) return;
  lastWindowFocusSyncMs = now;
  await calendarSyncFromApiQuiet();
}

async function applyGoogleCalendarToGrid() {
  try {
    const s = normalizeCalendarState(await invoke("get_calendar_state"));
    setUseGoogleCalendar(s.connected);
    updateSyncHint(s);
    updateGoogleButtons(s);
    if (s.connected) {
      const events = await invoke("get_cached_calendar_events");
      setRemoteTasksByIso(eventsToByIso(events));
    } else {
      setRemoteTasksByIso({});
    }
    renderAll();
    restartCalendarAutoSync();
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
        "Para OAuth: GOOGLE_OAUTH_CLIENT_ID (e, se o cliente for tipo Web na Google, GOOGLE_OAUTH_CLIENT_SECRET). Redirect: http://127.0.0.1:17892/callback (ver docs/GOOGLE-CALENDAR-FASE2.md).";
    } else {
      hint.classList.add("hidden");
      hint.textContent = "";
    }
  }

  signIn.disabled = !cfg || state.connected;
  sync.disabled = !cfg || !state.connected;
  disc.disabled = !cfg || !state.connected;

  const flushQ = document.getElementById("btn-google-flush-queue");
  if (flushQ) {
    const show =
      cfg && state.connected && (state.pendingMutationsCount ?? 0) > 0;
    flushQ.classList.toggle("hidden", !show);
    flushQ.disabled = !show;
  }

  const row = document.getElementById("calendar-actions");
  if (row) {
    if (cfg && state.connected) row.classList.add("has-google-session");
    else row.classList.remove("has-google-session");
  }

  const createBlock = document.getElementById("create-google-event-block");
  if (createBlock) {
    const show = cfg && state.connected;
    createBlock.classList.toggle("hidden", !show);
    createBlock.querySelectorAll("input, button").forEach((el) => {
      el.disabled = !show;
    });
  }

  const newBar = document.getElementById("btn-new-gc-event");
  if (newBar) {
    const show = cfg && state.connected;
    newBar.classList.toggle("hidden", !show);
    newBar.disabled = !show;
  }
}

/** Atualiza texto da fila offline (barra de definições + dica de sync). */
async function syncMaximizeButton() {
  const b = document.getElementById("btn-maximize-window");
  if (!b) return;
  try {
    const on = await invoke("window_is_maximized");
    b.classList.toggle("active", Boolean(on));
    b.setAttribute("aria-pressed", on ? "true" : "false");
    b.title = on ? "Restaurar tamanho da janela" : "Janela em tela cheia (maximizar)";
  } catch (_) {
    /* ignorar */
  }
}

async function bumpCalendarHintsFromBackend() {
  try {
    const s = normalizeCalendarState(await invoke("get_calendar_state"));
    updateSyncHint(s);
    await refreshCalendarStateLine();
  } catch (_) {
    /* ignorar */
  }
}

async function refreshCalendarStateLine() {
  const el = document.getElementById("calendar-state-line");
  if (!el) return;
  try {
    const s = normalizeCalendarState(await invoke("get_calendar_state"));
    updateSyncHint(s);
    updateGoogleButtons(s);
    const db = s.dbReady ? "Base local (cache) pronta." : "Base local indisponível.";
    if (!s.clientIdConfigured) {
      el.textContent = `${db} Client OAuth não configurado — a grelha usa dados de demonstração.`;
    } else if (!s.connected) {
      el.textContent = `${db} Client ID configurado. Inicia sessão com Google para substituir o demo na grelha.`;
    } else {
      const q = s.pendingMutationsCount ?? 0;
      const queue =
        q > 0
          ? ` Fila offline: ${q} pendente(s) — Sincronizar tenta enviar.`
          : "";
      el.textContent = `${db} Conta Google ligada — a grelha mostra eventos em cache (sincroniza para atualizar).${queue}`;
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
    void refreshAutostartCheckbox();
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

  document.getElementById("btn-maximize-window")?.addEventListener("click", async () => {
    try {
      const on = await invoke("window_toggle_maximized");
      const b = document.getElementById("btn-maximize-window");
      if (b) {
        b.classList.toggle("active", Boolean(on));
        b.setAttribute("aria-pressed", on ? "true" : "false");
        b.title = on ? "Restaurar tamanho da janela" : "Janela em tela cheia (maximizar)";
      }
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-minimize")?.addEventListener("click", async () => {
    try {
      await invoke("window_minimize");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-close-main")?.addEventListener("click", async () => {
    try {
      await invoke("window_close_main");
    } catch (e) {
      alert(e?.message || String(e));
    }
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

  document.getElementById("select-view-layout")?.addEventListener("change", async (e) => {
    appConfig.viewMode =
      /** @type {HTMLSelectElement} */ (e.target).value === "app" ? "app" : "widget";
    applyViewMode(appConfig.viewMode);
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

  document.getElementById("btn-google-sign-in")?.addEventListener("click", async () => {
    setCalendarOAuthMessage("");
    try {
      await invoke("google_calendar_sign_in");
      setCalendarOAuthMessage(
        "Sessão iniciada. Usa o botão «Sincronizar agora» logo abaixo para trazer os eventos.",
      );
      await refreshCalendarStateLine();
      applyGoogleButtonsSignedIn();
      requestAnimationFrame(() => {
        document
          .getElementById("btn-google-sync")
          ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      });
      await applyGoogleCalendarToGrid();
    } catch (e) {
      setCalendarOAuthMessage(e?.message || String(e));
    }
  });

  document.getElementById("btn-google-sync")?.addEventListener("click", async () => {
    setCalendarOAuthMessage("");
    try {
      const n = await invoke("google_calendar_sync");
      setCalendarOAuthMessage(`Sincronizados ${n} evento(s).`);
      await applyGoogleCalendarToGrid();
      await bumpCalendarHintsFromBackend();
    } catch (e) {
      setCalendarOAuthMessage(e?.message || String(e));
    }
  });

  document.getElementById("btn-google-flush-queue")?.addEventListener("click", async () => {
    setCalendarOAuthMessage("");
    try {
      const n = await invoke("google_calendar_flush_offline_queue");
      await applyGoogleCalendarToGrid();
      if (n > 0) {
        setCalendarOAuthMessage(`Fila offline: enviadas ${n} alteração(ões).`);
      }
      await bumpCalendarHintsFromBackend();
    } catch (e) {
      setCalendarOAuthMessage(e?.message || String(e));
    }
  });

  document.getElementById("btn-google-disconnect")?.addEventListener("click", async () => {
    setCalendarOAuthMessage("");
    try {
      await invoke("google_calendar_disconnect");
      await refreshCalendarStateLine();
      await applyGoogleCalendarToGrid();
    } catch (e) {
      setCalendarOAuthMessage(e?.message || String(e));
    }
  });

  document.getElementById("btn-open-data-folder")?.addEventListener("click", async () => {
    try {
      await invoke("open_app_local_data_folder");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-open-config-folder")?.addEventListener("click", async () => {
    try {
      await invoke("open_app_config_folder");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-reset-window-layout")?.addEventListener("click", async () => {
    if (
      !confirm(
        "Repor a janela ao tamanho inicial (380×520) e apagar a posição guardada?",
      )
    )
      return;
    try {
      await invoke("reset_saved_window_layout");
    } catch (e) {
      alert(e?.message || String(e));
    }
  });

  document.getElementById("btn-open-gc-editor")?.addEventListener("click", () => {
    setCalendarOAuthMessage("");
    openEventEditor(null);
  });

  document.getElementById("btn-new-gc-event")?.addEventListener("click", () => {
    openEventEditor(null);
  });

  document.getElementById("select-auto-sync")?.addEventListener("change", async (e) => {
    appConfig.autoSyncMinutes = Number(/** @type {HTMLSelectElement} */ (e.target).value) || 0;
    await persistConfig();
    restartCalendarAutoSync();
  });

  document.getElementById("chk-close-to-tray")?.addEventListener("change", async (e) => {
    appConfig.closeToTray = /** @type {HTMLInputElement} */ (e.target).checked;
    await persistConfig();
  });

  document.getElementById("chk-start-windows")?.addEventListener("change", async (e) => {
    const input = /** @type {HTMLInputElement} */ (e.target);
    const want = input.checked;
    try {
      await invoke("autostart_set", { enabled: want });
    } catch (err) {
      alert(err?.message || String(err));
      input.checked = !want;
    }
  });

  window.addEventListener("focus", () => {
    void syncCalendarOnWindowFocusThrottled();
    void syncMaximizeButton();
  });

  document.getElementById("gc-allday")?.addEventListener("change", () => {
    syncGcAlldayUi();
  });

  document.getElementById("gc-start-date")?.addEventListener("change", () => {
    clampGcEndDateToStart();
  });

  document.getElementById("event-editor-backdrop")?.addEventListener("click", closeEventEditor);
  document.getElementById("gc-btn-close")?.addEventListener("click", closeEventEditor);

  document.getElementById("gc-btn-save")?.addEventListener("click", async () => {
    try {
      const p = readGoogleCalendarEditorForm();
      const ext = readEventExtensions();
      const eventId = document.getElementById("edit-event-id")?.value?.trim();
      const dl = descriptionLocationForPayload(
        p.description,
        p.location,
        Boolean(eventId),
      );
      if (!eventId) {
        await invoke("google_calendar_create_event", {
          payload: {
            summary: p.summary,
            allDay: p.allDay,
            startIso: p.startIso,
            endIso: p.endIso,
            description: dl.description,
            location: dl.location,
            extensions: ext,
          },
        });
      } else {
        const calendarId =
          document.getElementById("edit-calendar-id")?.value || "primary";
        await invoke("google_calendar_update_event", {
          payload: {
            calendarId,
            eventId,
            summary: p.summary,
            allDay: p.allDay,
            startIso: p.startIso,
            endIso: p.endIso,
            description: dl.description,
            location: dl.location,
            extensions: ext,
          },
        });
      }
      closeEventEditor();
      await applyGoogleCalendarToGrid();
    } catch (e) {
      alert(e?.message || String(e));
      await bumpCalendarHintsFromBackend();
    }
  });

  document.getElementById("gc-menu-delete")?.addEventListener("click", async () => {
    const title =
      document.getElementById("gc-title")?.value?.trim() || "este evento";
    if (!confirm(`Apagar "${title}" no Google Calendar?`)) return;
    try {
      const calendarId =
        document.getElementById("edit-calendar-id")?.value || "primary";
      const eventId = document.getElementById("edit-event-id")?.value;
      if (!eventId) return;
      await invoke("google_calendar_delete_event", {
        payload: { calendarId, eventId },
      });
      closeEventEditor();
      await applyGoogleCalendarToGrid();
    } catch (e) {
      alert(e?.message || String(e));
      await bumpCalendarHintsFromBackend();
    }
  });

  document.getElementById("gc-tz-fake")?.addEventListener("click", (e) => {
    e.preventDefault();
    alert("O fuso segue o relógio do sistema. Ajusta nas definições do Windows.");
  });
  document.getElementById("gc-tz-fake")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.target.click();
    }
  });

  document.getElementById("gc-reminders-default")?.addEventListener("change", () => {
    syncGcReminderUi();
  });

  document.getElementById("gc-meet-open")?.addEventListener("click", (e) => {
    const a = /** @type {HTMLAnchorElement} */ (e.currentTarget);
    if (a.getAttribute("href") === "#") e.preventDefault();
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    const ov = document.getElementById("event-editor-overlay");
    if (ov && !ov.classList.contains("hidden")) closeEventEditor();
  });

  if (window.matchMedia) {
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", () => {
        if (appConfig.theme === "system") applyTheme("system");
      });
  }

  const viewMonthEl = document.getElementById("view-month");
  if (viewMonthEl && typeof ResizeObserver !== "undefined") {
    new ResizeObserver(() => scheduleMonthRelayoutFromResize()).observe(viewMonthEl);
  }

  loadConfig();
});
