import { useEffect, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { CAPS_LOCK, comboLabel, comboOf, DEFAULTS, sameCombo, SECTIONS, type Action, type Combo } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";

/** Asks before photos go to the trash. */
export function TrashDialog() {
  const pending = useStore((s) => s.trashPending);
  const platform = useStore((s) => s.platform);
  const count = pending?.length ?? 0;
  const bin = platform === "windows" ? "Recycle Bin" : "Trash";

  return (
    <AlertDialog open={pending !== null} onOpenChange={(open) => !open && useStore.getState().set("trashPending", null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{count === 1 ? `Move Photo to ${bin}?` : `Move ${count} Photos to ${bin}?`}</AlertDialogTitle>
          <AlertDialogDescription>
            {count === 1 ? "The photo" : "The selected photos"} will be moved to the {bin}, along with any JPEG and sidecar file beside {count === 1 ? "it" : "them"}.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={() => void useStore.getState().confirmTrash()}>
            Move to {bin}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/**
 * Every shortcut, and a way to change each.
 *
 * Click one and press the keys wanted. If those keys already do something
 * else, that other shortcut is left with nothing rather than both firing.
 */
export function ShortcutsDialog({ bindings, onChange, onRecording }: { bindings: Record<Action, Combo>; onChange: (b: Record<Action, Combo>) => void; onRecording: (on: boolean) => void }) {
  const open = useStore((s) => s.shortcutsOpen);
  const [recording, setRecording] = useState<Action | null>(null);

  useEffect(() => onRecording(recording !== null), [recording, onRecording]);
  useEffect(() => {
    if (!open) setRecording(null);
  }, [open]);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      // Caps Lock is a switch: it reports going on as a press and going off
      // as a release, and never both. Either is the key being chosen — but
      // only for speed cull, the one thing a switch makes sense for.
      const caps = e.code === CAPS_LOCK;
      if (caps ? recording !== "toggleSpeedCull" : e.type !== "keydown") return;
      // A modifier on its own is the start of a combination, not one.
      if (/^(Meta|Control|Alt|Shift)/.test(e.code)) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.code !== "Escape") {
        const combo = caps ? { code: CAPS_LOCK } : comboOf(e);
        const next = { ...bindings };
        for (const other of Object.keys(next) as Action[]) {
          // An impossible key, so the displaced shortcut matches nothing.
          if (other !== recording && sameCombo(next[other], combo)) next[other] = { code: "" };
        }
        next[recording] = combo;
        onChange(next);
      }
      setRecording(null);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKey, true);
    };
  }, [recording, bindings, onChange]);

  return (
    <Dialog open={open} onOpenChange={(o) => useStore.getState().set("shortcutsOpen", o)}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle>Keyboard Shortcuts</DialogTitle>
          <DialogDescription>Click a shortcut, then press the keys you want for it. Escape cancels.</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-x-8 gap-y-5">
          {SECTIONS.map((section) => (
            <div key={section.title} className="space-y-1.5">
              <div className="text-[10px] font-bold uppercase text-[#737373]">{section.title}</div>
              {section.actions.map(([action, label]) => (
                <div key={action} className="flex items-center justify-between gap-3 text-[13px] text-[#d9d9d9]">
                  {label}
                  <button
                    type="button"
                    onClick={() => setRecording(recording === action ? null : action)}
                    className={cn(
                      "min-w-12 rounded border px-2 py-0.5 font-mono text-[11px]",
                      recording === action ? "border-brand bg-brand/15 text-brand" : "border-[#3a3a44] bg-white/[0.04] text-[#bfbfbf] hover:border-[#595959]",
                    )}
                  >
                    {recording === action ? "press…" : bindings[action].code ? comboLabel(bindings[action]) : "—"}
                  </button>
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="flex justify-end">
          <Button variant="outline" size="sm" onClick={() => onChange({ ...DEFAULTS })}>
            Reset to Defaults
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** Tells the photographer something they must not miss, and waits to be acknowledged. */
export function NoticeDialog() {
  const notice = useStore((s) => s.notice);
  return (
    <AlertDialog open={notice !== null} onOpenChange={(open) => !open && useStore.getState().set("notice", null)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{notice?.title}</AlertDialogTitle>
          <AlertDialogDescription className="whitespace-pre-line">{notice?.body}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction>OK</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
