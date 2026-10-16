import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { posix } from "node:path";

export const CUDA_LIBRARIES = ["libcuda.so.1", "libcudart.so.13", "libcublas.so.13", "libcublasLt.so.13", "libcurand.so.10", "libcudnn.so.9"];
/** Build-time hint; the native runtime verifies device initialization and loading. */
export function detectCuda(platform, arch, env, run = execFileSync, exists = existsSync) {
  if (platform !== "linux" || arch !== "x64") return { available: false, reason: "CUDA requires Linux x86_64" };
  const options = { encoding: "utf8", timeout: 2000, stdio: ["ignore", "pipe", "pipe"], env };
  try {
    const devices = run("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"], options).trim();
    if (!devices) return { available: false, reason: "No NVIDIA GPU detected" };
    let libraries = "";
    try { libraries = run(exists("/sbin/ldconfig") ? "/sbin/ldconfig" : "ldconfig", ["-p"], options); } catch { /* Custom library paths can still supply the runtime. */ }
    const directories = (env.LD_LIBRARY_PATH ?? "").split(":").filter(Boolean);
    const missing = CUDA_LIBRARIES.filter(name => !libraries.split("\n").some(line => line.trim().startsWith(`${name} `) && line.includes("x86-64")) && !directories.some(dir => exists(posix.join(dir, name))));
    return missing.length ? { available: false, reason: `Missing CUDA libraries: ${missing.join(", ")}` } : { available: true, reason: devices };
  } catch { return { available: false, reason: "NVIDIA driver/device unavailable (nvidia-smi)" }; }
}
