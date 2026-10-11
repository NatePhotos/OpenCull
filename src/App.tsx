import { useCallback, useEffect, useRef, useState } from "react";

import { CullingView } from "@/components/cull/CullingView";
import { NoticeDialog, SettingsDialog, ShortcutsDialog, TrashDialog } from "@/components/cull/Dialogs";
import { ShipDialog } from "@/components/cull/ShipDialog";
import { IngestView } from "@/components/ingest/IngestView";
import { HomeView } from "@/components/home/HomeView";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api, inTauri } from "@/lib/api";
import { installShortcuts, loadBindings, saveBindings, type Action, type Combo } from "@/lib/shortcuts";
import { baseName } from "@/lib/format";
import { useStore } from "@/lib/store";

export default function App() {
  const open = useStore((s) => s.session !== null);
  const ingesting = useStore((s) => s.ingestOpen);
  const [bindings, setBindings] = useState(loadBindings);
  const [dropping, setDropping] = useState(false);

  // Read through refs by the key listener, which is attached once and must
  // still see a shortcut changed a moment ago.
  const live = useRef(bindings);
  live.current = bindings;
  const recording = useRef(false);

  useEffect(() => {
    void useStore.getState().init();
    return installShortcuts(
      () => live.current,
      () => recording.current,
    );
  }, []);

  const changeBindings = useCallback((next: Record<Action, Combo>) => {
    setBindings(next);
    saveBindings(next);
  }, []);
  const setRecording = useCallback((on: boolean) => {
    recording.current = on;
  }, []);

  // A folder dropped on the window opens. The system hands over paths, which
  // a web page on its own is never told, so this goes through the app shell.
  useEffect(() => {
    if (!inTauri) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((event) => {
          const { type } = event.payload;
          if (type === "enter" || type === "over") setDropping(true);
          else setDropping(false);
          if (type === "drop" && event.payload.paths[0]) void useStore.getState().openFolder(event.payload.paths[0]);
        })
        .then((unlisten) => {
          if (cancelled) unlisten();
          else stop = unlisten;
        }),
    );
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  // A rating that could not be written into a JPEG. Said once for each
  // folder: on a card locked against writing it is true of every photo, and
  // a warning for each would be a wall of them.
  useEffect(() => {
    let told: number | null = null;
    return api.onJpegNotWritten((path, why) => {
      const s = useStore.getState();
      const folder = s.session?.id ?? -1;
      if (told === folder) return;
      told = folder;
      s.set("notice", {
        title: "A rating could not be written into the JPEG",
        body: `${baseName(path)}: ${why}.\n\nThe rating is still saved beside the photo, so OpenCull has it, but other programs will not see it on the JPEG. This is only said once for each folder. To stop writing into JPEGs altogether, turn it off in Settings, under Culling.`,
      });
    });
  }, []);

  // A rating that could not be saved at all: its sidecar could not be
  // written. Said once for each folder, for the same reason as above, but
  // never left unsaid — the photographer believes it is saved.
  useEffect(() => {
    let told: number | null = null;
    return api.onMarksNotSaved((path, why) => {
      const s = useStore.getState();
      const folder = s.session?.id ?? -1;
      if (told === folder) return;
      told = folder;
      s.set("notice", {
        title: "A rating could not be saved",
        body: `${baseName(path)}: ${why}.\n\nThe mark shows here, but it is not saved beside the photo, so it will be gone when the folder is opened again and no other program will see it. Check that the folder can be written to — a card's lock switch, a full disk, a network drive that has gone. This is only said once for each folder.`,
      });
    });
  }, []);

  // Quitting is refused while photos are being copied off a card, moved or
  // shipped. Say why, or it looks as though the app has stopped listening.
  useEffect(
    () =>
      api.onQuitRefused(() =>
        useStore.getState().set("notice", {
          title: "Photos are still being copied",
          body: "Quitting now would leave some photos copied and others not. Wait for the copy, move or ship to finish — or, for a card, press Cancel Copy first: the photo in hand is finished either way.",
        }),
      ),
    [],
  );

  // Anything that goes wrong in the interface is reported to the app's log,
  // where it can be read without opening developer tools.
  useEffect(() => {
    const onError = (e: ErrorEvent) => api.report(`error: ${e.message} (${e.filename}:${e.lineno})`);
    const onRejection = (e: PromiseRejectionEvent) => api.report(`unhandled: ${String(e.reason)}`);
    const onBlocked = (e: SecurityPolicyViolationEvent) => api.report(`blocked by policy: ${e.violatedDirective} ${e.blockedURI}`);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    document.addEventListener("securitypolicyviolation", onBlocked);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
      document.removeEventListener("securitypolicyviolation", onBlocked);
    };
  }, []);

  return (
    <TooltipProvider delayDuration={500}>
      {open ? <CullingView /> : <HomeView dropping={dropping} />}
      {ingesting && <IngestView />}
      <TrashDialog />
      <SettingsDialog />
      <ShipDialog />
      <NoticeDialog />
      <ShortcutsDialog bindings={bindings} onChange={changeBindings} onRecording={setRecording} />
    </TooltipProvider>
  );
}
