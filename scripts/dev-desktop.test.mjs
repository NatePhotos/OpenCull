import { test } from "node:test";
import assert from "node:assert/strict";
import { desktopSpec, reuseFrontend } from "./dev-desktop.mjs";

test("ordinary desktop launch preserves native platform defaults", () => {
  for (const platform of ["darwin", "win32", "linux"]) {
    const env = { PATH: "/bin" };
    assert.deepEqual(desktopSpec(platform, "x64", env, ["--no-watch"]), { args: ["dev", "--no-watch"], env });
  }
});
test("Linux graphics and CUDA flags combine without consuming Tauri or app arguments", () => {
  const env = { PATH: "/bin" };
  const spec = desktopSpec("linux", "x64", env, ["--nvidia", "--cuda", "--no-watch", "--", "--software"]);
  assert.deepEqual(spec.args, ["dev", "--features", "cuda", "--no-watch", "--", "--software"]);
  assert.equal(spec.env.OPENCULL_GRAPHICS, "nvidia");
  assert.equal(spec.env.OPENCULL_AI, "cuda");
  assert.deepEqual(env, { PATH: "/bin" });
});
test("invalid or unsupported profiles fail before spawning Tauri", () => {
  assert.throws(() => desktopSpec("linux", "x64", {}, ["--nvidia", "--software"]), /Choose one/);
  assert.throws(() => desktopSpec("darwin", "arm64", {}, ["--nvidia"]), /Linux-only/);
  assert.throws(() => desktopSpec("win32", "x64", {}, ["--cuda"]), /Linux x86_64/);
});

test("a free port leaves frontend startup owned by Tauri", async () => {
  const request = async () => { throw new TypeError("fetch failed", { cause: { code: "ECONNREFUSED" } }); };
  assert.deepEqual(await reuseFrontend(["dev"], request), { args: ["dev"], reused: false });
});
test("an existing OpenCull frontend is reused without consuming app arguments", async () => {
  const request = async () => new Response('<title>OpenCull</title><script src="/@vite/client"></script>');
  const original = ["dev", "--no-watch", "--", "--graphics=nvidia"];
  const result = await reuseFrontend(original, request);
  assert.equal(result.reused, true);
  assert.deepEqual(result.args, ["dev", "--no-watch", "--config", '{"build":{"beforeDevCommand":""}}', "--", "--graphics=nvidia"]);
  assert.equal(original.length, 4);
});
test("foreign servers and unreachable occupied ports report a clear error before starting Cargo", async () => {
  await assert.rejects(reuseFrontend(["dev"], async () => new Response('<title>Other app</title>')), /another server/);
  await assert.rejects(reuseFrontend(["dev"], async () => { throw new DOMException("Timed out", "TimeoutError"); }), /could not be checked/);
});

test("automatic CUDA follows detection only on supported targets and respects CPU overrides", () => {
  assert.deepEqual(desktopSpec("linux", "x64", {}, [], true).args, ["dev", "--features", "cuda"]);
  assert.equal(desktopSpec("linux", "x64", {}, [], true).env.OPENCULL_AI, "auto");
  assert.deepEqual(desktopSpec("linux", "x64", {}, ["--cpu"], true), {args:["dev"],env:{OPENCULL_AI:"cpu"}});
  assert.deepEqual(desktopSpec("linux", "x64", {OPENCULL_AI:"cpu"}, [], true).args, ["dev"]);
  for (const [platform, arch] of [["darwin","arm64"],["win32","x64"],["linux","arm64"]]) assert.deepEqual(desktopSpec(platform,arch,{},[],true).args,["dev"]);
  assert.throws(()=>desktopSpec("linux","x64",{},["--cuda","--cpu"],true),/Choose one/);
});
