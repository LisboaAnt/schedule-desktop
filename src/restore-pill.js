const { invoke } = window.__TAURI__.core;

document.getElementById("restore-btn")?.addEventListener("click", async () => {
  try {
    await invoke("restore_desktop_wallpaper_mode");
  } catch (e) {
    console.error(e);
  }
});
