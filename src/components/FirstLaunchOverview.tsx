import { useEffect } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "./ui/dialog";
import { Button } from "./ui/button";
import { comboLabel, type Action, type Combo } from "@/lib/shortcuts";
import { useStore } from "@/lib/store";
import { save } from "@/lib/prefs";

const COMMON: [Action, string][] = [
  ["nextImage", "Next photo"], ["previousImage", "Previous photo"],
  ["nextGroup", "Next group"], ["pick", "Pick"], ["reject", "Reject"],
  ["unflag", "Clear flag"], ["rate1", "1 star"],
  ["zoomCentre", "Zoom to face"], ["zoomToggle", "Zoom to centre"],
  ["toggleFilmstrip", "Show/hide filmstrip"], ["viewGrid", "Grid view"], ["undo", "Undo"],
];
export function FirstLaunchOverview({ bindings }: { bindings: Record<Action, Combo> }) {
  const open = useStore((s) => s.overviewOpen);
  const selfTest = useStore((s) => s.selfTest);
  useEffect(() => { if (selfTest) useStore.getState().set("overviewOpen", false); }, [selfTest]);
  const close = () => { save("onboardingSeen", true); useStore.getState().set("overviewOpen", false); };
  return <Dialog open={open && !selfTest} onOpenChange={(next) => { if (!next) close(); }}>
    <DialogContent className="max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] overflow-y-auto sm:max-w-3xl sm:p-6">
      <DialogHeader><DialogTitle>Get started with OpenCull</DialogTitle></DialogHeader>
      <ol className="grid gap-3 sm:grid-cols-3">
        <li className="rounded-lg border border-border bg-card p-4"><h2 className="text-base font-semibold">1. Bring in a shoot</h2><p className="mt-2 text-sm leading-relaxed text-muted-foreground">Open a folder, or use Ingest from Card. Add multiple cards to review one shoot before copying anything.</p></li>
        <li className="rounded-lg border border-border bg-card p-4"><h2 className="text-base font-semibold">2. Find the keepers</h2><p className="mt-2 text-sm leading-relaxed text-muted-foreground">Move through photos or stacks. Pick, reject and rate frames, then zoom in to check details. Flags alone never delete originals.</p></li>
        <li className="rounded-lg border border-border bg-card p-4"><h2 className="text-base font-semibold">3. Finish the review</h2><p className="mt-2 text-sm leading-relaxed text-muted-foreground">For cards, choose what to copy or import in place. Use Ship for export and copying. Resume saved reviews from Recent Sessions.</p></li>
      </ol>
      <section aria-label="Common keyboard shortcuts" className="rounded-lg border border-border bg-card p-4">
        <h2 className="text-base font-semibold">Common keyboard shortcuts</h2>
        <dl className="mt-3 grid gap-x-8 gap-y-3 sm:grid-cols-2">{COMMON.map(([action, label]) => <div key={action} className="flex items-center justify-between gap-3 text-sm"><dt>{label}</dt><dd><kbd className="rounded border border-input bg-background px-2 py-1 font-mono">{bindings[action].code ? comboLabel(bindings[action]) : "Unassigned"}</kbd></dd></div>)}</dl>
        <p className="mt-4 text-sm leading-relaxed text-muted-foreground">These reflect your current bindings. Edit shortcuts or reopen this overview in Settings → Keyboard. Configure the interface in Settings → Design; Wisp provides a ready-made culling setup.</p>
      </section>
      <div className="flex justify-end"><Button onClick={close}>Start culling</Button></div>
    </DialogContent>
  </Dialog>;
}
