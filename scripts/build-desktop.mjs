import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { desktopSpec } from "./dev-desktop.mjs";
import { cudaBundleSpec } from "./cuda-bundle.mjs";
import { detectCuda } from "./detect-cuda.mjs";

await import("./check-node.mjs");
try {
  const args = process.argv.slice(2);
  const initial = desktopSpec(process.platform, process.arch, process.env, args);
  const automatic = !initial.env.OPENCULL_AI || initial.env.OPENCULL_AI === "auto";
  const detection = automatic ? detectCuda(process.platform, process.arch, process.env) : { available: false };
  const spec = cudaBundleSpec(process.platform, desktopSpec(process.platform, process.arch, process.env, args, detection.available));
  spec.args[0] = "build";
  if (process.platform === "linux" && process.arch === "x64") console.log(`[ai] Desktop build: ${spec.args.includes("cuda") ? "CUDA enabled" : "CPU"}`);
  const cli = fileURLToPath(new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url));
  const child = spawn(process.execPath, [cli, ...spec.args], { stdio: "inherit", env: spec.env });
  child.on("error", error => { console.error(error.message); process.exitCode = 1; });
  child.on("exit", code => { process.exitCode = code ?? 1; });
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => child.kill(signal));
} catch (error) { console.error(error.message); process.exitCode = 1; }
