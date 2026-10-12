import { useEffect, useRef, useState } from "react";

import { AlertDialog, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { useStore } from "@/lib/store";

/**
 * Offers a newer version, and installs it if that is wanted.
 *
 * A version found as the app opens is offered once, and only on the home
 * screen: never over a folder being culled, where a window arriving
 * unasked would take whatever key was pressed next.
 */
export function UpdateDialog() {
  const found = useStore((s) => s.updateFound);
  const open = useStore((s) => s.updateOpen);
  const windows = useStore((s) => s.platform === "windows");
  const idle = useStore((s) => s.session === null && !s.loading && !s.ingestOpen && !s.settingsOpen && !s.shortcutsOpen && !s.notice);
  const offered = useRef(false);
  /** How much has come down, 0 to 1, or `null` when nothing is being installed. */
  const [progress, setProgress] = useState<number | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    if (!found || !idle || offered.current || open) return;
    offered.current = true;
    useStore.getState().set("updateOpen", true);
  }, [found, idle, open]);
  // However it came to be open, it has now been offered.
  useEffect(() => {
    if (open) offered.current = true;
  }, [open]);

  const installing = progress !== null;
  const close = () => {
    if (installing) return;
    setFailed(null);
    useStore.getState().set("updateOpen", false);
  };

  const install = async () => {
    setFailed(null);
    setProgress(0);
    try {
      await api.updateInstall((done, total) => setProgress(total ? Math.min(1, done / total) : 0));
      // Reached only where the app is not restarted from under this.
    } catch (e) {
      setFailed(String(e));
    }
    setProgress(null);
  };

  return (
    <AlertDialog open={open && found !== null} onOpenChange={(o) => !o && close()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>OpenCull {found?.version} is available</AlertDialogTitle>
          <AlertDialogDescription className="whitespace-pre-line">
            {failed
              ? `The update could not be installed:\n${failed}\n\nIt can be downloaded and installed by hand instead. Your settings and ratings are kept either way.`
              : `You have ${found?.current}. Installing downloads the new version, ${windows ? "runs its installer" : "puts it in place of this one"} and reopens OpenCull. Your settings and ratings are kept.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {installing && (
          <div className="space-y-1.5">
            <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.08]">
              <span className="block h-full rounded-full bg-[#59a6ff] transition-[width] duration-150" style={{ width: `${Math.round(progress * 100)}%` }} />
            </div>
            <div className="text-xs text-[#8c8c8c]">{progress < 1 ? `Downloading… ${Math.round(progress * 100)}%` : "Installing…"}</div>
          </div>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={installing}>Later</AlertDialogCancel>
          <Button variant="outline" disabled={installing} onClick={() => void api.openReleasePage(failed ? null : (found?.version ?? null)).catch(() => {})}>
            {failed ? "Open the Download Page" : "What's New"}
          </Button>
          <Button disabled={installing} onClick={() => void install()}>
            {installing ? "Installing…" : failed ? "Try Again" : "Install and Reopen"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
