/**
 * Copia `target/<profile>/agenda-watchdog.exe` → `src-tauri/binaries/agenda-watchdog-<triple>.exe`
 * (requerido por `bundle.externalBin` antes do `cargo build` do pacote principal).
 *
 * Uso: `node scripts/copy-watchdog.mjs --profile=debug|release`
 * Defeito: release (para `prepare-watchdog-release`).
 */
import { copyFileSync, mkdirSync, existsSync } from "fs";
import { execSync } from "child_process";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

if (process.platform !== "win32") {
  console.log("copy-watchdog: ignorado (não Windows).");
  process.exit(0);
}

let profile = "release";
const arg = process.argv.find((a) => a.startsWith("--profile="));
if (arg) {
  profile = arg.split("=")[1] || "release";
} else if (process.argv.includes("--debug")) {
  profile = "debug";
}

const triple = execSync("rustc --print host-tuple", { encoding: "utf8" }).trim();
if (!triple) {
  console.error("copy-watchdog: falha a obter host-tuple do rustc.");
  process.exit(1);
}

const src = join(root, "target", profile, "agenda-watchdog.exe");
const outDir = join(root, "src-tauri", "binaries");
const dst = join(outDir, `agenda-watchdog-${triple}.exe`);

if (!existsSync(src)) {
  console.error(`copy-watchdog: ficheiro em falta (${profile}):`);
  console.error(src);
  process.exit(1);
}

mkdirSync(outDir, { recursive: true });
copyFileSync(src, dst);
console.log("copy-watchdog:", dst);
