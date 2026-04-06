/**
 * Lança agenda-watchdog.exe (debug) com child = calendario-app.exe (release).
 * O UI em release embute o frontend; não precisa do servidor do `tauri dev`.
 * Ver docs/COMO-RODAR.md — «Vigia em desenvolvimento local».
 */
import { spawn } from "child_process";
import { existsSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const wd = join(root, "target", "debug", "agenda-watchdog.exe");
const child = join(root, "target", "release", "calendario-app.exe");

if (process.platform !== "win32") {
  console.error("run-watchdog-release: só Windows.");
  process.exit(1);
}
if (!existsSync(wd)) {
  console.error("Em falta:", wd, "— corre: cargo build -p agenda-watchdog");
  process.exit(1);
}
if (!existsSync(child)) {
  console.error("Em falta:", child, "— corre: cargo build -p calendario-app --release");
  process.exit(1);
}

const p = spawn(wd, ["--child", child], { stdio: "inherit", cwd: root });
p.on("exit", (code, signal) => {
  if (signal) process.exit(1);
  process.exit(code ?? 0);
});
