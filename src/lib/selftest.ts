// Runs the app against a real folder with nobody at the controls, and says
// what happened. Started with `--selftest <folder>`; see `Launch` in the
// backend for why it exists.
//
// It goes through the interface's own code — the same store actions a click
// or a key press would call — so what it proves is that the app works, not
// that a test harness does. It never marks a photo: the folder it is pointed
// at is someone's real work, and a check has no business writing to it.

import { api, bucket, FIT_SIZES, THUMB_SIZES } from "./api";
import { hold, isHeld } from "./held";
import { groupOf, useStore } from "./store";
import type { AiResult } from "./types";

const say = (line: string) => api.report(`selftest: ${line}`);

/** Loads an address the way an `<img>` would, and times it. */
function load(url: string): Promise<{ ms: number; width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const started = performance.now();
    const img = new Image();
    img.onload = () => resolve({ ms: performance.now() - started, width: img.naturalWidth, height: img.naturalHeight });
    img.onerror = () => reject(new Error(`did not load: ${url.slice(0, 90)}`));
    img.src = url;
  });
}

/**
 * Waits `ms` on the backend's clock rather than this window's. The self-test
 * runs in a window nobody is looking at, and the system slows the timers of
 * those — MEASURED: a 120 ms pause took 317 ms.
 */
const wait = (ms: number) => api.pause(ms);

async function until(done: () => boolean, what: string, timeoutMs = 60_000): Promise<void> {
  const started = performance.now();
  while (!done()) {
    if (performance.now() - started > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await wait(10);
  }
}

const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)] ?? 0;

export async function runSelfTest(folder: string): Promise<void> {
  try {
    const s = useStore.getState;

    let t = performance.now();
    await until(() => s().session !== null && !s().loading, "the folder to be listed");
    const count = s().photos.length;
    say(`listed ${count} photos in ${Math.round(performance.now() - t)} ms  (${folder})`);
    if (count === 0) throw new Error("the folder holds no photos");

    // Whether the folder came up in the order it was shot, straight away.
    const opened = s();
    const firstByName = opened.photos[0];
    const firstShown = opened.photos[opened.order[0]];
    say(
      opened.session?.timed
        ? `opened already in shooting order: first frame shown is ${firstShown.name}${firstShown.index === 0 ? "" : ` (by filename it would have been ${firstByName.name})`}`
        : "opened in filename order — the capture times were not all read in time",
    );
    const openedOrder = opened.order;

    await until(() => s().metaProgress >= 1, "ratings and EXIF");
    const photos = s().photos;
    const withTime = photos.filter((p) => p.exif?.captureMs != null).length;
    const withSize = photos.filter((p) => p.fullWidth > 0).length;
    const bursts = new Set(photos.map((p) => p.group).filter((g) => g !== null)).size;
    const rated = photos.filter((p) => p.rating > 0).length;
    say(`read metadata in ${Math.round(performance.now() - t)} ms: ${withTime} with a capture time, ${withSize} with a size, ${rated} rated, ${bursts} bursts`);
    if (opened.session?.timed) {
      // The full read must not move anything the quick one had placed.
      const settled = s().order;
      const moved = settled.findIndex((index, at) => index !== openedOrder[at]);
      say(moved < 0 ? "  the order did not change when the rest of the metadata arrived" : `  THE ORDER CHANGED at position ${moved} when the rest of the metadata arrived`);
      if (moved >= 0) throw new Error("the quick capture times and the full metadata disagree about the order");
    }
    if (withSize === 0) throw new Error("no photo reported a size, so no preview was found in any of them");

    // Thumbnails, as the filmstrip asks for them.
    const sample = s().view.slice(0, 40).map((i) => photos[i]);
    t = performance.now();
    const thumbs = await Promise.all(sample.map((p) => load(api.url("thumb", p, THUMB_SIZES[0]))));
    say(`${thumbs.length} thumbnails in ${Math.round(performance.now() - t)} ms, ${thumbs[0].width}x${thumbs[0].height}`);

    // The loupe: one photo at a time, the way arrow keys reach them. The
    // first is cold; the rest have been looked ahead to by the time they
    // are asked for, which is the case that has to be fast.
    const size = bucket(FIT_SIZES, 2400);
    const cold = await load(api.url("fit", sample[0], size));
    say(`first loupe preview in ${Math.round(cold.ms)} ms, ${cold.width}x${cold.height}`);

    const warm: number[] = [];
    void api.prefetch(sample.slice(1, 7).map((p) => ({ path: p.display, version: p.version })), size);
    for (let i = 1; i < Math.min(sample.length, 25); i++) {
      const ahead = sample.slice(i + 1, i + 7).map((p) => ({ path: p.display, version: p.version }));
      void api.prefetch(ahead, size);
      // About as long as someone looks at a frame while flipping through.
      await wait(120);
      warm.push((await load(api.url("fit", sample[i], size))).ms);
    }
    say(`next ${warm.length} loupe previews, looked ahead to: median ${median(warm).toFixed(1)} ms, slowest ${Math.max(...warm).toFixed(1)} ms`);

    // A jump to a photo nothing has been prepared for: what shows first, and
    // how long the sharp one takes with the machine otherwise quiet.
    const view = s().view;
    const at = (fraction: number) => photos[view[Math.floor(view.length * fraction)]];
    const far = at(0.6);
    const screen = await load(api.url("screen", far)).catch(() => null);
    say(screen ? `jump to a cold photo: the camera's screen-sized preview shows in ${screen.ms.toFixed(1)} ms, ${screen.width}x${screen.height}` : "jump to a cold photo: this file has no screen-sized preview");
    const quiet = await load(api.url("fit", far, size));
    say(`  its fit rendering, made from nothing: ${Math.round(quiet.ms)} ms`);

    // The same, while the folder is being prepared in the background — the
    // case where getting ahead must not get in the way.
    const stretch = view.slice(40, 240).map((i) => photos[i]);
    if (stretch.length >= 50) {
      t = performance.now();
      const queued = await api.warm(stretch.map((p) => ({ path: p.display, version: p.version })), size);
      await wait(900); // long enough for preparing to be in full swing
      const busy = await load(api.url("fit", at(0.8), size));
      say(`  the same jump while ${await api.warmRemaining()} previews are being prepared: ${Math.round(busy.ms)} ms`);

      for (let waited = 0; (await api.warmRemaining()) > 0; waited += 50) {
        if (waited > 60_000) throw new Error("preparing previews ahead never finished");
        await wait(50);
      }
      const seconds = (performance.now() - t) / 1000;
      const jump = await load(api.url("fit", stretch[Math.floor(stretch.length / 2)], size));
      say(`prepared ${queued} previews ahead in ${seconds.toFixed(1)} s, pauses included; a jump into them then takes ${jump.ms.toFixed(1)} ms`);
    } else {
      say("too few photos here to test preparing ahead");
    }

    // Full size, and that a frame shot on its side comes back the right way up.
    const full = await load(api.url("full", sample[0]));
    say(`full size in ${Math.round(full.ms)} ms, ${full.width}x${full.height}`);
    const upright = photos.find((p) => p.fullHeight > p.fullWidth);
    if (upright) {
      const turned = await load(api.url("full", upright));
      const ok = turned.height > turned.width;
      say(`portrait frame ${upright.name}: full size arrives ${turned.width}x${turned.height} — ${ok ? "upright" : "ON ITS SIDE"}`);
      if (!ok) throw new Error("a portrait frame was not turned upright");
      const fit = await load(api.url("fit", upright, size));
      if (fit.height <= fit.width) throw new Error("a portrait frame's loupe preview was not turned upright");
    } else {
      say("no portrait frame in this folder to check orientation with");
    }

    // What a held arrow key leans on: screen-sized pictures decoded in the
    // view before they are stepped onto. Thirty photos a second is one every
    // 33 ms, so this has to come in well under that apiece.
    const run = view.slice(Math.floor(view.length * 0.3), Math.floor(view.length * 0.3) + 10).map((i) => photos[i]);
    const urls = run.map((p) => api.url("screen", p));
    t = performance.now();
    urls.forEach(hold);
    await until(() => urls.every(isHeld), "screen-sized pictures to be decoded in the view", 10_000).then(
      () => say(`${urls.length} screen-sized pictures decoded and held in ${Math.round(performance.now() - t)} ms, ${((performance.now() - t) / urls.length).toFixed(1)} ms each`),
      () => say(`screen-sized pictures held: only ${urls.filter(isHeld).length} of ${urls.length} — a held arrow key will show thumbnails for the rest`),
    );

    // And what zooming leans on: the full-size picture decoded while a photo
    // is rested on. This is the wait that getting it ready early removes.
    const big = new Image();
    big.src = api.url("full", run[0]);
    t = performance.now();
    await big.decode();
    say(`full-size picture decoded in the view in ${Math.round(performance.now() - t)} ms — the wait a zoom no longer has`);

    // And what the face panel leans on for the small faces of a group: that
    // picture, already decoded, drawn from again. Asking for it a second
    // time must cost nothing, and cutting a panel's worth of faces out of it
    // must take a small part of a frame — set beside the same cuts from the
    // window-sized rendering, which is what the panel draws them from first.
    //
    // Timed on a photo the camera held level. The measure used here makes
    // an upright copy of a turned photo before it cuts anything — MEASURED:
    // 118 ms, once, for 25 megapixels — which drawing a picture in the
    // panel does not do, so a turned photo would time the measure and not
    // the panel.
    const second = new Image();
    second.src = big.src;
    t = performance.now();
    await second.decode();
    const shared = performance.now() - t;
    if (shared > 50) throw new Error(`a second use of the held full-size picture took ${Math.round(shared)} ms: it is being decoded again`);
    const cuts = async (from: HTMLImageElement) => {
      const began = performance.now();
      for (let cell = 0; cell < 12; cell++) {
        // A face with the air round it, a twelfth of the frame wide and
        // somewhere different each time, made the size of a cell on a
        // sharp screen.
        const w = Math.round(from.naturalWidth * 0.08);
        const piece = await createImageBitmap(from, Math.round(from.naturalWidth * (0.05 + 0.07 * cell)), Math.round(from.naturalHeight * (0.1 + 0.05 * cell)), w, Math.round(w * 1.1), { resizeWidth: 300, resizeHeight: 330 });
        piece.close();
      }
      return performance.now() - began;
    };
    const level = photos.find((p) => p.fullWidth > p.fullHeight);
    if (level) {
      const [whole, sized] = [new Image(), new Image()];
      whole.src = api.url("full", level);
      sized.src = api.url("fit", level, size);
      await Promise.all([whole.decode(), sized.decode()]);
      const [fromFull, fromFit] = [await cuts(whole), await cuts(sized)];
      say(`the held full-size picture asked for again was ready in ${shared.toFixed(1)} ms; twelve face-sized cuts from a ${whole.naturalWidth}x${whole.naturalHeight} picture in ${fromFull.toFixed(1)} ms, against ${fromFit.toFixed(1)} ms from the window-sized rendering`);
      // MEASURED: 12 to 14 ms for the twelve, 2 to 3 from the rendering.
      if (fromFull > 40) throw new Error(`cutting a panel of faces from the full-size picture took ${Math.round(fromFull)} ms, where 12 to 14 is usual`);
    } else {
      say(`the held full-size picture asked for again was ready in ${shared.toFixed(1)} ms; no level photo here to time cuts from`);
    }

    // Copying photos out, the way the right-click menu and Ship do — into
    // this run's own temporary folder, which is deleted when it ends. Nothing
    // is moved, and nothing is opened in any editing program.
    const scratch = (await api.appInfo()).cacheDir.replace(/[\\/][^\\/]+$/, "");
    const pair = sample.slice(0, 2);
    const originals = pair.map((p) => p.path);
    const both = pair.map((p) => ({ path: p.path, display: p.display }));
    const clear = await api.shipConflicts(both, scratch);
    let heard = 0;
    t = performance.now();
    const sent = await api.ship(both, scratch, false, false, (done) => (heard = done));
    const took = performance.now() - t;
    await until(() => heard === pair.length, "Ship to report its progress", 5_000);
    const arrived = (await api.pathsExist(sent.placed)).every(Boolean);
    const untouched = (await api.pathsExist(originals)).every(Boolean);
    // A RAW's JPEG goes with it, under the name the RAW landed on.
    const extension = (path: string) => path.match(/\.[^.\\/]+$/)?.[0] ?? "";
    const jpegs = pair.flatMap((p, i) => (p.display === p.path ? [] : [sent.placed[i].slice(0, sent.placed[i].length - extension(sent.placed[i]).length) + extension(p.display)]));
    const paired = (await api.pathsExist(jpegs)).every(Boolean);
    const taken = await api.shipConflicts(both, scratch);
    const skipped = await api.ship(both, scratch, false, true, () => {});
    const beside = await api.transferPhotos([{ path: pair[0].path, display: pair[0].display }], scratch, false);
    const renamed = beside.placed[0] !== sent.placed[0] && (await api.pathsExist(beside.placed))[0];
    say(
      `ship: ${sent.placed.length} photos and ${jpegs.length} JPEGs beside them copied in ${Math.round(took)} ms; originals ${untouched ? "untouched" : "MISSING"}; a second ship ${skipped.skipped === pair.length ? "skips" : "DOES NOT SKIP"} them; a copy beside them ${renamed ? "takes a new name" : "DID NOT GET A NEW NAME"}`,
    );
    if (clear !== 0 || sent.failed.length > 0 || sent.sidecarFailed.length > 0 || sent.moved.length > 0 || !arrived) throw new Error(`ship did not copy cleanly: ${sent.failed.join("; ") || "files missing"}`);
    if (!untouched) throw new Error("a copy took the original with it");
    if (!paired) throw new Error("ship left a RAW's JPEG behind");
    if (taken !== pair.length || skipped.skipped !== pair.length || skipped.placed.join() !== originals.join()) throw new Error("ship did not notice the names already taken");
    if (!renamed || beside.failed.length > 0) throw new Error("a copy onto a taken name did not go alongside under a new one");
    // Framing: a crop and a turn saved for one of the copies just made in
    // the temporary folder — never for a photo of the photographer's — to
    // show the whole route from here to the sidecar holds.
    const copy = sent.placed[0];
    const made = pair[0];
    if (made.fullWidth > 0 && made.framable) {
      const sidecar = copy.replace(/\.[^.\\/]+$/, ".xmp");
      const [had] = await api.pathsExist([sidecar]);
      const middle = { cx: made.fullHeight / 2, cy: made.fullWidth / 2, w: made.fullHeight / 2, h: made.fullWidth / 2, angle: 3 };
      await api.setFraming([{ path: copy, into: [], framing: { turn: 6, crop: middle }, upright: [made.fullWidth, made.fullHeight], seed: { rating: 0, flag: "none", color: "none" } }]);
      await api.flushMarks();
      const [has] = await api.pathsExist([sidecar]);
      say(`framing: a crop and a quarter turn saved beside a copy${had ? " that already had a sidecar" : ""}; its sidecar is ${has ? "there" : "MISSING"}`);
      if (!has) throw new Error("a crop was saved and no sidecar was written for it");
    } else {
      say("framing: the first photo here cannot be framed, so saving a crop was not tried");
    }

    say(`editing programs found here: ${(await api.editingApps()).filter((a) => a.installed).map((a) => a.name).join(", ")}`);

    {
      // Ingest, with this folder standing in for a card: list it, read its
      // capture times, and copy two photos into this run's temporary folder.
      const card = await api.ingestScan(folder);
      if (card.length === 0) throw new Error("ingest found no photos in a folder that has them");
      const sampled = card.slice(0, 64);
      const read: number[] = [];
      t = performance.now();
      await api.ingestDetails(sampled.map((p) => p.path), (batch) => batch.forEach((d) => d.captureMs !== null && read.push(d.index)));
      const detailMs = performance.now() - t;
      const into = `${scratch}/ingest`;
      const two = card.slice(0, 2).map(({ path, display }) => ({ path, display }));
      const none = await api.ingestConflicts(two, into);
      const reports: { done: number; settled: number; total: number }[] = [];
      t = performance.now();
      const ingested = await api.ingestCopy(two, into, "uniqueName", (p) => reports.push({ done: p.done, settled: p.bytesSettled, total: p.bytesTotal }));
      const ingestMs = performance.now() - t;
      await until(() => reports.some((r) => r.done === two.length), "ingest to report that it had finished", 5_000);
      const last = reports[reports.length - 1];
      const forward = reports.every((r, i) => i === 0 || r.settled >= reports[i - 1].settled);
      const landed = (await api.pathsExist(two.map((i) => `${into}/${i.path.split("/").pop()}`))).every(Boolean);
      const again = await api.ingestConflicts(two, into);
      const passedOver = await api.ingestCopy(two, into, "skip", () => {});
      say(
        `ingest: ${card.length} photos listed; ${read.length} of ${sampled.length} capture times read in ${Math.round(detailMs)} ms; 2 copied in ${Math.round(ingestMs)} ms at ${Math.round(last.total / 1e6 / (ingestMs / 1000))} MB/s over ${reports.length} reports; a second ingest finds ${again} already there`,
      );
      if (none !== 0 || ingested.status !== "success" || !landed) throw new Error(`ingest did not copy cleanly: ${ingested.message ?? "files missing"}`);
      if (!forward || last.settled !== last.total || last.total === 0) throw new Error("ingest's progress did not run forward to the end");
      if (again !== two.length || passedOver.status !== "success") throw new Error("ingest did not notice the photos already copied");
      if (!(await api.pathsExist(two.map((i) => i.path))).every(Boolean)) throw new Error("ingest took a photo off the card");
      say(`cards and drives attached: ${(await api.ingestVolumes()).map((v) => `${v.name}${v.cameraCard ? " (camera card)" : ""}`).join(", ") || "none"}`);
    }

    // Face analysis, through the same route the app uses: two dozen photos,
    // then the same again, which must come back from memory.
    {
      const some = s().order.slice(0, 24);
      const seen = new Map<number, { faces: number; score: number | null }>();
      const whole = new Map<number, AiResult>();
      const basic = new Map<number, AiResult>();
      let bad = "";
      const scan = (advanced = true) =>
        new Promise<number>((resolve, reject) => {
          const began = performance.now();
          api
            .aiScan(s().session!.id, some, advanced, (e) => {
              if (e.type === "done") return resolve(performance.now() - began);
              for (const item of e.items) {
                if (!advanced) {
                  basic.set(item.index, item);
                  continue;
                }
                seen.set(item.index, { faces: item.faces.length, score: item.score });
                whole.set(item.index, item);
                for (const f of item.faces) {
                  if (!(f.w > 0 && f.h > 0 && f.x > -0.2 && f.y > -0.2 && f.x + f.w < 1.2 && f.y + f.h < 1.2)) bad = `a face box outside its photo: ${JSON.stringify(f)}`;
                }
                if (item.faces.length > 0 && item.score === null) bad = "a photo with faces was given no mark";
              }
            })
            .catch(reject);
        });
      const first = await scan();
      const found = [...seen.values()];
      const again = await scan();
      say(
        `faces: ${found.length} photos analysed in ${(first / 1000).toFixed(1)} s (${Math.round(first / found.length)} ms each); ${found.reduce((n, f) => n + f.faces, 0)} faces, ${found.filter((f) => f.faces > 0).length} photos with any; the same again from memory in ${Math.round(again)} ms`,
      );
      if (found.length !== some.length) throw new Error(`face analysis returned ${found.length} of ${some.length} photos`);
      if (bad) throw new Error(bad);
      if (again > first / 3 && again > 300) throw new Error("a second analysis was not served from memory");
      // The same photos by the rules of the last release: found in the same
      // places, kept in a memory of their own, and no face given a state.
      const byBasic = await scan(false);
      if (basic.size !== some.length) throw new Error(`the basic rules returned ${basic.size} of ${some.length} photos`);
      const told = [...basic.values()].flatMap((r) => r.faces).filter((f) => f.state !== null).length;
      if (told > 0) throw new Error(`the basic rules gave ${told} faces a state`);
      const sameFaces = [...basic].every(([index, r]) => r.faces.length === whole.get(index)!.faces.length);
      if (!sameFaces) throw new Error("the basic rules found different faces from the advanced ones");
      const moved = [...basic].filter(([index, r]) => Math.abs((r.score ?? 0) - (whole.get(index)!.score ?? 0)) >= 1).length;
      const named = [...whole.values()].flatMap((r) => r.faces).filter((f) => f.state !== null).length;
      say(`faces, by the basic rules: the same ${some.length} photos in ${(byBasic / 1000).toFixed(1)} s; ${moved} of them marked differently; the advanced rules name the state of ${named} faces, the basic of none`);
      const kept = await scan();
      if (kept > first / 3 && kept > 300) throw new Error("what the advanced rules found was lost by running the basic ones");
      // Pointing the scan at the photo being looked at: only that the
      // route is there. What it does with it is tested where it lives.
      await api.aiLookAt(some.slice(0, 3));
      // The frames of a burst marked against each other. Any run of photos
      // will do to show the route works; real bursts are not needed for that.
      const run = some.slice(0, 6).map((index) => ({ ...whole.get(index)!, time: s().photos[index].captureMs, burst: 1, upright: null }));
      const [marked] = await api.aiCompare([run], true);
      if (marked.length !== run.length || marked.some((m, at) => m.eyes.length !== run[at].faces.length)) throw new Error("burst comparison did not answer for every face");
      const shares = marked.flatMap((m) => m.eyes).filter((e): e is number => e !== null);
      if (shares.some((e) => !(e > 0 && e <= 1))) throw new Error(`an eye share outside 0 to 1: ${shares.join(", ")}`);
      say(`burst comparison: ${run.length} frames, ${shares.length} of ${run.reduce((n, r) => n + r.faces.length, 0)} faces followed, least open ${shares.length ? Math.round(Math.min(...shares) * 100) + "%" : "none"}`);

      // And the way the interface does it: handed what was found, it works
      // out for itself which photos were taken together and ranks them.
      s().addAnalysis([...whole].map(([index, r]) => ({ ...r, index })));
      if (s().aiDone !== some.length) throw new Error("the interface did not take in the analysis");
      await new Promise((done) => setTimeout(done, 500));
      const ranked = some.filter((index) => s().ranks[index] !== undefined);
      const bad_rank = ranked.find((index) => s().compared[index] === undefined || s().ranks[index]!.rank > s().ranks[index]!.count);
      if (bad_rank !== undefined) throw new Error(`photo ${bad_rank} has a rank that makes no sense`);
      // Scenes follow the poses, and go back to bursts when that is turned off.
      const scenes = () => {
        let n = 0;
        some.forEach((index, at) => {
          const [g, before] = [groupOf(s(), index), at > 0 ? groupOf(s(), some[at - 1]) : null];
          if (g === null || g !== before) n += 1;
        });
        return n;
      };
      // The setting is the photographer's own and is put back whatever happens.
      const chosen = s().smartGroups;
      const settle = async (on: boolean) => {
        s().set("smartGroups", on);
        s().regroup();
        await new Promise((done) => setTimeout(done, 500));
        return scenes();
      };
      let smart: number, plain: number, back: number;
      try {
        smart = await settle(true);
        plain = await settle(false);
        back = await settle(true);
      } finally {
        await settle(chosen);
      }
      if (back !== smart) throw new Error("turning intelligent grouping off and on again did not come back to the same scenes");
      if (smart > plain) throw new Error("intelligent grouping made more scenes, not fewer");
      say(`scenes: those ${some.length} photos are ${smart} scenes with intelligent grouping and ${plain} without`);
      say(`poses: of ${some.length} photos, ${some.filter((index) => s().compared[index] !== undefined).length} were compared with those taken around them and ${ranked.length} ranked, ${ranked.filter((index) => s().ranks[index]!.rank === 1).length} of them the best of their pose`);
    }

    // The interface's own navigation.
    const first = s().current;
    s().next();
    s().next();
    const moved = s().current !== first;
    s().previousGroup();
    say(`navigation ${moved ? "moves" : "DOES NOT MOVE"}; histogram ${(await api.histogram(sample[0].display, sample[0].version)).luma.length === 256 ? "ok" : "BAD"}`);
    if (!moved) throw new Error("the arrow keys' action did not change the photo");

    await (await import("./linux-features.selftest")).checkLinuxFeatures(folder, scratch);
    say("PASSED");
    api.finishSelfTest(true);
  } catch (e) {
    say(`FAILED: ${e instanceof Error ? e.message : String(e)}`);
    api.finishSelfTest(false);
  }
}
