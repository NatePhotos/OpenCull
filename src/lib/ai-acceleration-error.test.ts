import { expect, it } from "vitest";
import { accelerationError } from "./ai-acceleration-error";

it("explains VRAM exhaustion and distinguishes a busy GPU", () => {
  expect(
    accelerationError({
      requested: true,
      enabled: false,
      failure: "Failed to allocate memory",
      failureKind: "memory",
    }),
  ).toMatchObject({
    title: "Not enough GPU memory (VRAM)",
    body: expect.stringContaining("switched to CPU"),
  });

  expect(
    accelerationError({
      requested: true,
      enabled: false,
      failure: "CUDA-capable devices are busy or unavailable",
      failureKind: "unavailable",
    })?.title,
  ).toBe("GPU busy or unavailable");
});

it("shows no CUDA alert on unsupported platforms, CPU launches, or healthy workers", () => {
  expect(accelerationError(null)).toBeNull();
  expect(
    accelerationError({
      requested: false,
      enabled: false,
      failure: "old failure",
      failureKind: "memory",
    }),
  ).toBeNull();
  expect(
    accelerationError({
      requested: true,
      enabled: true,
      failure: null,
      failureKind: null,
    }),
  ).toBeNull();
});

it("shows a generic actionable message for other CUDA runtime failures", () => {
  expect(
    accelerationError({
      requested: true,
      enabled: false,
      failure: "The CUDA provider could not be loaded",
      failureKind: "runtime",
    }),
  ).toMatchObject({ title: "GPU acceleration failed" });
});
