import { useHomeAppearance } from "@/components/HomeLayoutSettings";
import { useInterfaceScale } from "@/components/InterfaceScaleSettings";
import { FirstLaunchOverview } from "@/components/FirstLaunchOverview";
import { useCallback, useEffect, useRef, useState } from "react";

import { useAppFont } from "@/components/FontSettings";
import { useAppTheme } from "@/components/ThemeSettings";

import { CullingView } from "@/components/cull/CullingView";
import { NoticeDialog, SettingsDialog, TrashDialog } from "@/components/cull/Dialogs";
import { ShipDialog } from "@/components/cull/ShipDialog";
import { IngestView } from "@/components/ingest/IngestView";
import { HomeView } from "@/components/home/HomeView";
import { UpdateDialog } from "@/components/UpdateDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api, inTauri } from "@/lib/api";
import { installShortcuts, loadBindings, saveBindings, type Action, type Combo } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";

export default function App() {
  useAppFont();
  useAppTheme();
  useHomeAppearance();
  useInterfaceScale();
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
        body: `${path.split(/[\\/]/).pop()}: ${why}.\n\nThe rating is still saved beside the photo, so OpenCull has it, but other programs will not see it on the JPEG. This is only said once for each folder. To stop writing into JPEGs altogether, turn it off in Settings, under Culling.`,
      });
    });
  }, []);

  // A rating, flag or colour that could not be saved anywhere. Said once
  // for each folder, as above: on a locked card it is true of every photo.
  useEffect(() => {
    let told: number | null = null;
    return api.onMarksNotSaved((path, why) => {
      const s = useStore.getState();
      const folder = s.session?.id ?? -1;
      if (told === folder) return;
      told = folder;
      s.set("notice", {
        title: "A rating could not be saved",
        body: `${path.split(/[\\/]/).pop()}: ${why}.\n\nThe mark shows here for now, but it was not saved, so it will be gone when the folder is next opened. Check that the folder can be written to: a card's lock switch, a full disk, a drive that has gone away. If the photo has a sidecar file that cannot be read, it is left untouched so nothing in it is lost. This is only said once for each folder.`,
      });
    });
  }, []);

  // A crop or a turn that could not be saved. Said once for each folder, as
  // above, and for the worse of the two if both happen: not saved at all
  // outranks not written into a JPEG.
  useEffect(() => {
    let told: { folder: number; lost: boolean } | null = null;
    return api.onFramingNotSaved((path, why, lost) => {
      const s = useStore.getState();
      const folder = s.session?.id ?? -1;
      if (told?.folder === folder && (told.lost || !lost)) return;
      told = { folder, lost };
      const name = path.split(/[\\/]/).pop();
      s.set(
        "notice",
        lost
          ? {
              title: "A crop could not be saved",
              body: `${name}: ${why}.\n\nThe photo shows as you framed it for now, but that was not saved, so it will be as it was when the folder is next opened. Check that the folder can be written to: a card's lock switch, a full disk, a drive that has gone away. This is only said once for each folder.`,
            }
          : {
              title: "A crop could not be written into the JPEG",
              body: `${name}: ${why}.\n\nThe crop is still saved beside the photo, so OpenCull has it, but other programs will not see it on the JPEG. This is only said once for each folder.`,
            },
      );
    });
  }, []);

  // Quitting is refused while photos are coming off a card. Say why, or it
  // looks as though the app has stopped listening.
  useEffect(
    () =>
      api.onQuitRefused(() =>
        useStore.getState().set("notice", {
          title: "Photos are still being copied",
          body: "Quitting now would leave part of the shoot uncopied. Wait for the copy to finish, or press Cancel Copy first — the photo in hand is finished either way.",
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
      <SettingsDialog bindings={bindings} onChange={changeBindings} onRecording={setRecording} />
      <ShipDialog />
      <NoticeDialog />
      <UpdateDialog />
      <FirstLaunchOverview bindings={bindings} />
    </TooltipProvider>
  );
}
