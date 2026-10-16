import type { AiAccelerationInfo } from "./api";

export function accelerationError(info: AiAccelerationInfo | null) {
  if (!info?.requested || info.enabled || !info.failure) return null;

  const title =
    info.failureKind === "memory"
      ? "Not enough GPU memory (VRAM)"
      : info.failureKind === "unavailable"
        ? "GPU busy or unavailable"
        : "GPU acceleration failed";

  return {
    title,
    body: "AI has switched to CPU processing. Close other GPU-heavy apps, then restart OpenCull with CUDA to retry.",
    details: info.failure,
  };
}
