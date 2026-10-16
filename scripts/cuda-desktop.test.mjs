import { test } from "node:test";
import assert from "node:assert/strict";
import { launchSpec } from "./cuda-desktop.mjs";

test("CUDA launcher rejects macOS, Windows and ARM before starting a process", () => {
  for (const [platform, arch] of [["darwin", "arm64"], ["darwin", "x64"], ["win32", "x64"], ["linux", "arm64"]]) {
    assert.throws(() => launchSpec(platform, arch, {}, []), /Linux x86_64/);
  }
});
test("Linux launch opts into CUDA while leaving normal launches unchanged", () => {
  const env = { PATH: "/bin", OPENCULL_GRAPHICS: "software" };
  const spec = launchSpec("linux", "x64", env, ["--no-watch"]);
  assert.deepEqual(spec.args, ["dev", "--features", "cuda", "--no-watch"]);
  assert.equal(spec.env.OPENCULL_AI, "cuda");
  assert.equal(spec.env.OPENCULL_GRAPHICS, "software");
  assert.equal(env.OPENCULL_AI, undefined);
});

import { detectCuda, CUDA_LIBRARIES } from "./detect-cuda.mjs";
test("CUDA detection needs a working GPU and every 64-bit library", () => {
  const libraries=CUDA_LIBRARIES.map(name=>`${name} (libc6,x86-64) => /lib/${name}`).join("\n");
  const run=command=>command==="nvidia-smi"?"NVIDIA test GPU\n":libraries;
  assert.equal(detectCuda("linux","x64",{},run,()=>false).available,true);
  assert.equal(detectCuda("linux","x64",{},command=>command==="nvidia-smi"?"GPU":libraries.replace("libcudnn.so.9 (libc6,x86-64)","libcudnn.so.9 (libc6)"),()=>false).available,false);
  assert.equal(detectCuda("linux","x64",{},()=>{throw Error("driver missing")},()=>false).available,false);
  assert.equal(detectCuda("linux","x64",{},()=>"",()=>false).available,false);
  assert.equal(detectCuda("linux","x64",{LD_LIBRARY_PATH:"/custom/lib"},command=>command==="nvidia-smi"?"GPU":"",path=>path.startsWith("/custom/lib/")).available,true);
  for(const platform of ["darwin","win32"]) assert.equal(detectCuda(platform,"x64",{},()=>{throw Error("must not probe")}).available,false);
});

import { cudaBundleSpec } from "./cuda-bundle.mjs";
test("CUDA installers include matching providers next to the executable without bundling NVIDIA runtimes", () => {
  const original={args:["dev","--features","cuda"],env:{LINUXDEPLOY_EXCLUDED_LIBRARIES:"custom.so"}};
  const release=cudaBundleSpec("linux",original);
  const config=JSON.parse(release.args[release.args.indexOf("--config")+1]);
  for(const format of ["deb","rpm","appimage"]) {
    assert.equal(config.bundle.linux[format].files["/usr/bin/libonnxruntime_providers_cuda.so"],"target/release/libonnxruntime_providers_cuda.so");
    assert.equal(config.bundle.linux[format].files["/usr/bin/libonnxruntime_providers_shared.so"],"target/release/libonnxruntime_providers_shared.so");
  }
  assert.match(release.env.LINUXDEPLOY_EXCLUDED_LIBRARIES,/^custom.so;/);
  const debug=cudaBundleSpec("linux",{args:[...original.args,"--debug"],env:{}});
  assert.match(debug.args[debug.args.indexOf("--config")+1],/target\/debug\/libonnxruntime/);
  assert.equal(cudaBundleSpec("darwin",original),original);
  const cpu={args:["dev"],env:{}};assert.equal(cudaBundleSpec("linux",cpu),cpu);
  assert.equal(original.args.length,3);
});
