import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
const profile = process.argv[2];
if (!["nvidia", "compatibility", "software"].includes(profile) || process.platform !== "linux") {
  console.error("Graphics profiles are Linux-only. Use npm run desktop on macOS or Windows."); process.exit(1);
}
const cli = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
const child = spawn(process.execPath, [cli, "dev", ...process.argv.slice(3)], { stdio: "inherit", env: { ...process.env, OPENCULL_GRAPHICS: profile } });
child.on("error", (e) => { console.error(e.message); process.exitCode = 1; });
child.on("exit", (code) => { process.exitCode = code ?? 1; });
