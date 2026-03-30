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

/** @type {{ viewMode: string, theme: string, widgetOpacity: number }} */
let appConfig = {
  viewMode: "widget",
  theme: "dark",
  widgetOpacity: 1,
};

let viewYear = new Date().getFullYear();
let viewMonth = new Date().getMonth();

function padIso(y, m, d) {
  const mm = String(m + 1).padStart(2, "0");
  const dd = String(d).padStart(2, "0");
  return `${y}-${mm}-${dd}`;
}

function isToday(y, m, d) {
  const t = new Date();
  return t.getFullYear() === y && t.getMonth() === m && t.getDate() === d;
}

function renderWeekdays() {
  const row = document.getElementById("weekday-row");
  row.replaceChildren();
  for (const w of WEEKDAYS) {
    const el = document.createElement("div");
    el.textContent = w;
    row.append(el);
  }
}

function renderMonth() {
  const label = document.getElementById("month-label");
  label.textContent = `${MONTH_NAMES[viewMonth]} ${viewYear}`;

  const grid = document.getElementById("day-grid");
  grid.replaceChildren();

  const first = new Date(viewYear, viewMonth, 1);
  const startWeekday = (first.getDay() + 6) % 7;
  const daysInMonth = new Date(viewYear, viewMonth + 1, 0).getDate();
  const prevMonthDays = new Date(viewYear, viewMonth, 0).getDate();

  const totalCells = 42;
  let dayCounter = 1;
  let nextMonthDay = 1;

  for (let i = 0; i < totalCells; i++) {
    const cell = document.createElement("div");
    cell.className = "cell";

    let y = viewYear;
    let m = viewMonth;
    let d;

    if (i < startWeekday) {
      d = prevMonthDays - (startWeekday - 1 - i);
      m -= 1;
      if (m < 0) {
        m = 11;
        y -= 1;
      }
      cell.classList.add("other-month");
    } else if (dayCounter <= daysInMonth) {
      d = dayCounter;
      cell.classList.add("in-month");
      dayCounter += 1;
    } else {
      d = nextMonthDay;
      nextMonthDay += 1;
      m += 1;
      if (m > 11) {
        m = 0;
        y += 1;
      }
      cell.classList.add("other-month");
    }

    cell.textContent = String(d);
    cell.dataset.iso = padIso(y, m, d);
    if (isToday(y, m, d)) cell.classList.add("today");

    grid.append(cell);
  }
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
    };
  } catch (e) {
    console.warn("get_app_config", e);
  }
  applyTheme(appConfig.theme);
  applyViewMode(appConfig.viewMode);
  applyOpacity(appConfig.widgetOpacity);
  document.getElementById("select-theme").value =
    appConfig.theme === "system"
      ? "system"
      : appConfig.theme === "light"
        ? "light"
        : "dark";
  document.getElementById("range-opacity").value = String(
    appConfig.widgetOpacity,
  );
}

function showPanel(which) {
  const cal = document.querySelector(".calendar-panel");
  const set = document.getElementById("settings-panel");
  const bCal = document.getElementById("btn-cal");
  const bSet = document.getElementById("btn-settings");
  if (which === "settings") {
    cal.classList.add("hidden");
    set.classList.remove("hidden");
    bCal.classList.remove("active");
    bSet.classList.add("active");
  } else {
    cal.classList.remove("hidden");
    set.classList.add("hidden");
    bCal.classList.add("active");
    bSet.classList.remove("active");
  }
}

window.addEventListener("DOMContentLoaded", () => {
  renderWeekdays();
  renderMonth();

  document.getElementById("btn-prev").addEventListener("click", () => {
    viewMonth -= 1;
    if (viewMonth < 0) {
      viewMonth = 11;
      viewYear -= 1;
    }
    renderMonth();
  });

  document.getElementById("btn-next").addEventListener("click", () => {
    viewMonth += 1;
    if (viewMonth > 11) {
      viewMonth = 0;
      viewYear += 1;
    }
    renderMonth();
  });

  document.getElementById("btn-mode").addEventListener("click", async () => {
    appConfig.viewMode = appConfig.viewMode === "app" ? "widget" : "app";
    applyViewMode(appConfig.viewMode);
    await persistConfig();
  });

  document.getElementById("btn-cal").addEventListener("click", () => {
    showPanel("cal");
  });

  document.getElementById("btn-settings").addEventListener("click", () => {
    showPanel("settings");
  });

  document.getElementById("select-theme").addEventListener("change", async (e) => {
    appConfig.theme = e.target.value;
    applyTheme(appConfig.theme);
    await persistConfig();
  });

  document
    .getElementById("range-opacity")
    .addEventListener("input", async (e) => {
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
