import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

export function launchSpec(platform, arch, env, args) {
  if (platform !== "linux" || arch !== "x64") {
    throw new Error("The CUDA trial is Linux x86_64 only. Use npm run tauri -- dev on macOS or Windows.");
  }
  return { args: ["dev", "--features", "cuda", ...args], env: { ...env, OPENCULL_AI: "cuda" } };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const spec = launchSpec(process.platform, process.arch, process.env, process.argv.slice(2));
    const cli = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
    const child = spawn(process.execPath, [cli, ...spec.args], { stdio: "inherit", env: spec.env });
    child.on("error", (error) => { console.error(error.message); process.exitCode = 1; });
    child.on("exit", (code) => { process.exitCode = code ?? 1; });
    for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => child.kill(signal));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
