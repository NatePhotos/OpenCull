// Integration-only native checks. All copies land in the self-test scratch folder.
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { api } from "./api";
import { isVideo } from "./media";
import { newReview } from "./review";
import { copyReview, loadReview, saveReview } from "./review-api";
import { useStore } from "./store";

const assert = (ok: unknown, why: string) => { if (!ok) throw new Error(why); };
const say = (s: string) => api.report(`features: ${s}`);
async function until(check: () => boolean, message: string, limit = 10_000) {
  const start = performance.now();
  while (!check()) { if (performance.now() - start > limit) throw new Error(message); await api.pause(20); }
}

export async function checkLinuxFeatures(folder: string, scratch: string) {
  const s = useStore.getState;
  const configured = await invoke<Record<string, unknown>>("config_load", { legacy: {} });
  assert(configured && typeof configured === "object", "Native config did not load");
  if (typeof configured.theme === "string") assert(document.documentElement.dataset.theme === configured.theme, "Theme did not hydrate before startup");
  await document.fonts.load('14px "Space Grotesk"');
  assert(document.fonts.check('14px "Space Grotesk"'), "Offline Space Grotesk did not load");
  say("native configuration and offline font loaded");

  const sources = Array.isArray(configured.selfTestSources) && configured.selfTestSources.every((p) => typeof p === "string") ? configured.selfTestSources as string[] : [folder];
  const items = await api.ingestSources([...sources, sources[0]], false);
  if (sources.length > 1) assert(items.some((p) => p.path !== p.display), "Separate RAW/JPEG cards did not produce paired frames");
  assert(new Set(items.map((p) => p.path)).size === items.length, "Overlapping sources duplicated a file");
  const stills = items.filter((p) => !isVideo(p.path));
  assert(stills.length >= 2, "Feature checks need two still images");
  const before = await api.pathsExist(stills.map((p) => p.path.replace(/\.[^./\\]+$/, ".xmp")));
  const review = newReview(sources, stills.slice(0, 2));
  await s().openFolder(sources[0], review);
  await until(() => s().metaProgress >= 1, "Card metadata did not finish");
  s().setFlag("reject", [0]); s().setRating(4, [1]);
  await saveReview(s().review!);
  const restored = await loadReview();
  assert(restored?.decisions[stills[0].path].flag === "reject", "Local rejection did not persist");
  assert(restored?.decisions[stills[1].path].rating === 4, "Local rating did not persist");
  const copied = await copyReview([s().photos[1]], scratch, sources, () => {});
  assert(copied.failed.length === 0 && copied.sidecarFailed.length === 0, "Selected card copy failed");
  assert(Object.keys(copied.completed).length === 1, "Import copied the wrong membership");
  const after = await api.pathsExist(stills.map((p) => p.path.replace(/\.[^./\\]+$/, ".xmp")));
  assert(JSON.stringify(before) === JSON.stringify(after), "Review wrote a sidecar onto the source");
  const chosen = { ...s().review!, items: [stills[1]], chosen: [stills[1].path], stage: "in-place" as const };
  await s().openFolder(sources[0], chosen);
  await until(() => s().metaProgress >= 1, "In-place metadata did not finish");
  assert(s().photos.length === 1 && s().photos[0].rating === 4, "In-place import did not preserve selected membership and marks");
  say("card review, local decisions, selected copying, and in-place membership passed");

  s().set("settingsOpen", true);
  await until(() => !!document.querySelector('select[aria-label="Theme"]'), "Appearance settings did not render");
  const theme = document.querySelector<HTMLSelectElement>('select[aria-label="Theme"]')!;
  const original = theme.value; theme.value = "light"; theme.dispatchEvent(new Event("change", { bubbles: true }));
  await until(() => document.documentElement.dataset.theme === "light", "Theme control did not apply");
  theme.value = original; theme.dispatchEvent(new Event("change", { bubbles: true }));
  const layout = [...document.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((tab) => tab.textContent?.includes("Layout"));
  assert(layout, "Layout tab missing"); layout!.focus(); layout!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
  await until(() => !!document.querySelector('[aria-label="Face panel width"]'), "Layout size controls missing");
  s().set("settingsOpen", false);
  say("settings render and theme interaction passed");

  const video = items.find((p) => p.path.toLowerCase().endsWith(".mp4"));
  if (video) {
    s().set("ignoreVideos", false);
    await s().openFolder(sources[0], newReview(sources, [video]));
    await until(() => s().metaProgress >= 1, "Video metadata did not finish");
    const allowed = await invoke<string>("video_source", { path: video.path });
    const player = document.createElement("video");
    say(`codec support MP4=${player.canPlayType('video/mp4; codecs="avc1.64001e"')} WebM=${player.canPlayType('video/webm; codecs="vp8"')}`); player.muted = true; player.preload = "auto"; player.src = allowed.startsWith("http://127.0.0.1:") ? allowed : convertFileSrc(allowed); document.body.append(player);
    try {
      await new Promise<void>((resolve, reject) => {
        const timeout = window.setTimeout(() => reject(new Error("Video metadata timed out")), 10_000);
        player.onloadedmetadata = () => { clearTimeout(timeout); resolve(); };
        player.onerror = () => { clearTimeout(timeout); reject(new Error(`System could not decode the MP4 fixture (${player.error?.message})`)); };
      });
      assert(player.duration > 0, "Video has no duration");
      player.currentTime = Math.min(1, player.duration / 2);
      await until(() => !player.seeking, "Video seeking failed");
      await player.play(); await api.pause(200); player.pause(); assert(player.currentTime > 0, "Video did not advance");
      say(`native video metadata, seeking and playback passed (${player.duration.toFixed(1)}s)`);
    } finally { player.pause(); player.removeAttribute("src"); player.load(); player.remove(); }
  } else say("video fixture absent; native playback check skipped");
}
