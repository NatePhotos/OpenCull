import { ChevronDown, Eraser, Wand2 } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useStore } from "@/lib/store";
import type { ShootKind } from "@/lib/types";

const SHOOT_KINDS: { kind: ShootKind; label: string }[] = [
  { kind: "events", label: "Events" },
  { kind: "portraits", label: "Portraits & family" },
  { kind: "sports", label: "Sports & dance" },
];

const button = "flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-[11px] font-medium text-[var(--ui-text-cccccc)] hover:bg-white/10 disabled:opacity-40";

/** A small on/off with its name beside it, short enough for the toolbar. */
function Toggle({ id, label, checked, onChange, labelClass = "" }: { id: string; label: string; checked: boolean; onChange: (on: boolean) => void; labelClass?: string }) {
  return (
    // The name is also the tooltip, for windows too narrow to show it.
    <span className="flex shrink-0 items-center gap-1.5" title={label}>
      <Switch id={id} size="sm" checked={checked} onCheckedChange={onChange} aria-label={label} />
      <label htmlFor={id} className={`cursor-pointer whitespace-nowrap text-[11px] font-medium text-[var(--ui-text-cccccc)] ${labelClass}`}>
        {label}
      </label>
    </span>
  );
}

/**
 * The AI's controls, opened from the AI button: Advanced AI, and with it the
 * shoot type, Show why and Auto-label.
 */
export function AiBar() {
  const advanced = useStore((s) => s.aiAdvanced);
  const showWhy = useStore((s) => s.showWhy);
  const busy = useStore((s) => s.autocullBusy);
  // Until the scan and every burst comparison are done, some photos would go without their burst.
  const scanning = useStore((s) => s.aiRunning || s.comparing > 0);
  const judging = useStore((s) => s.judging > 0);
  const kind = useStore((s) => s.autoCullKind);
  const s = useStore.getState();
  const sep = <span className="h-[22px] w-px shrink-0 bg-[#383838]" />;

  return (
    // Never scrolls: when the window is too narrow the toolbar wraps it onto a line of its own,
    // and the button names give way to their icons first.
    <div className="ml-1.5 mr-1 flex shrink-0 items-center gap-1.5">
      <Toggle id="aibar-advanced" label="Advanced" labelClass="hidden min-[1400px]:inline" checked={advanced} onChange={(on) => s.set("aiAdvanced", on)} />
      {!advanced && <span className="shrink-0 whitespace-nowrap text-[11px] text-[var(--ui-text-808080)]">Shoot types and Auto-label need Advanced.</span>}
      {advanced && (
        <>
          {sep}
          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild>
                  <button type="button" className={button} disabled={busy !== null}>
                    <span>{SHOOT_KINDS.find((x) => x.kind === kind)?.label ?? "Events"}</span>
                    <ChevronDown className="size-3" />
                  </button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent>Shoot type: changes which faults count and how bursts are ranked</TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="start" className="w-48">
              <DropdownMenuLabel>Shoot type</DropdownMenuLabel>
              <DropdownMenuRadioGroup value={kind} onValueChange={(v) => void s.setAutoCullKind(v as ShootKind)}>
                {SHOOT_KINDS.map((x) => (
                  <DropdownMenuRadioItem key={x.kind} value={x.kind}>
                    {x.label}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
          {sep}
          <Toggle id="aibar-show-why" label="Show why" labelClass="hidden min-[1400px]:inline" checked={showWhy} onChange={(on) => s.set("showWhy", on)} />
          {sep}
          <Tooltip>
            <TooltipTrigger asChild>
              <button type="button" className={button} disabled={busy !== null || scanning || judging} onClick={() => void s.runAutoCull()}>
                <Wand2 className="size-3.5" />
                <span className="hidden min-[1400px]:inline">{busy === "colouring" ? "Labelling…" : "Auto-label"}</span>
              </button>
            </TooltipTrigger>
            <TooltipContent>{scanning ? "Waits for the AI scan to finish" : judging ? "Waits for the AI to finish judging" : "Colour-label this folder from the AI's judgement"}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <button type="button" className={button} disabled={busy !== null} onClick={() => void s.clearAutoCull()}>
                <Eraser className="size-3.5" />
                <span className="sr-only">{busy === "clearing" ? "Clearing…" : "Clear labels"}</span>
              </button>
            </TooltipTrigger>
            <TooltipContent>Clear labels: remove the colour labels Auto-label set</TooltipContent>
          </Tooltip>
        </>
      )}
    </div>
  );
}
