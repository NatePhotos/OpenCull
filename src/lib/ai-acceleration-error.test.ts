import { expect, it } from "vitest";
import { accelerationError } from "./ai-acceleration-error";
it("shows actionable VRAM exhaustion and accurately labels busy devices", () => {
  expect(accelerationError({requested:true,enabled:false,failure:"Failed to allocate memory",failureKind:"memory"})).toMatchObject({title:"Not enough GPU memory (VRAM)",body:expect.stringContaining("switched to CPU")});
  expect(accelerationError({requested:true,enabled:false,failure:"Device busy",failureKind:"unavailable"})?.title).toBe("GPU busy or unavailable");
});
it("shows no CUDA error on unsupported platforms, CPU launches or a healthy worker", () => {
  expect(accelerationError(null)).toBeNull();
  expect(accelerationError({requested:false,enabled:false,failure:"old",failureKind:"memory"})).toBeNull();
  expect(accelerationError({requested:true,enabled:true,failure:null,failureKind:null})).toBeNull();
});
