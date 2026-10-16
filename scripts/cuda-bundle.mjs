import { readFileSync } from "node:fs";

// CUDA runtimes belong to the host; package only ORT's matching provider files.
export const CUDA_EXCLUDED_LIBRARIES = "libcuda.so*;libcudart.so*;libcublas*.so*;libcurand.so*;libcudnn*.so*;libnvrtc*.so*;libnvidia*.so*";
export function cudaBundleSpec(platform, spec) {
  if (platform !== "linux" || !spec.args.includes("cuda")) return spec;
  const config = JSON.parse(readFileSync(new URL("../src-tauri/tauri.cuda.conf.json", import.meta.url), "utf8"));
  if (spec.args.includes("--debug")) {
    for (const format of Object.values(config.bundle.linux)) {
      for (const destination of Object.keys(format.files)) format.files[destination] = format.files[destination].replace("target/release/", "target/debug/");
    }
  }
  const args = [...spec.args];
  const separator = args.indexOf("--");
  args.splice(separator < 0 ? args.length : separator, 0, "--config", JSON.stringify(config));
  return { args, env: { ...spec.env, LINUXDEPLOY_EXCLUDED_LIBRARIES: [spec.env.LINUXDEPLOY_EXCLUDED_LIBRARIES, CUDA_EXCLUDED_LIBRARIES].filter(Boolean).join(";") } };
}
