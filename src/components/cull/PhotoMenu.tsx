import type { ReactNode } from "react";
import { Check } from "lucide-react";

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { api, logged } from "@/lib/api";
import { contextTargets, useStore } from "@/lib/store";
import { fileBrowserName } from "@/lib/format";
import { COLORS } from "@/lib/types";

/**
 * The right-click menu for a photo, wherever it is drawn.
 *
 * Acts on the whole selection when the photo clicked is part of it, and on
 * that one photo otherwise — the way a file browser's menu does.
 */
export function PhotoMenu({ index, children }: { index: number; children: ReactNode }) {
  const platform = useStore((s) => s.platform);
  const fileBrowser = fileBrowserName(platform);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-52">
        <Items index={index} fileBrowser={fileBrowser} />
      </ContextMenuContent>
    </ContextMenu>
  );
}

// Its own component so that nothing here is read from the store until the
// menu is actually open; a grid mounts one of these per cell.
function Items({ index, fileBrowser }: { index: number; fileBrowser: string }) {
  const s = useStore.getState();
  const targets = contextTargets(index);
  const photo = s.photos[index];
  const many = targets.length > 1;
  const subject = many ? `${targets.length} Photos` : "Photo";

  return (
    <>
      <ContextMenuItem onSelect={() => s.setFlag("pick", targets)}>Pick</ContextMenuItem>
      <ContextMenuItem onSelect={() => s.setFlag("reject", targets)}>Reject</ContextMenuItem>
      <ContextMenuItem onSelect={() => s.setFlag("none", targets)}>Unflag</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuSub>
        <ContextMenuSubTrigger>Rating</ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {[5, 4, 3, 2, 1, 0].map((n) => (
            <ContextMenuItem key={n} onSelect={() => s.setRating(n, targets)}>
              <span className="w-20 text-star">{n === 0 ? <span className="text-muted-foreground">None</span> : "★".repeat(n)}</span>
              {!many && photo.rating === n && <Check className="ml-auto size-3.5" />}
            </ContextMenuItem>
          ))}
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuSub>
        <ContextMenuSubTrigger>Colour</ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {COLORS.map((c) => (
            <ContextMenuItem key={c} onSelect={() => s.setColor(c, targets)}>
              <span className="size-3 rounded-full" style={{ background: `var(--opencull-${c})` }} />
              <span className="capitalize">{c}</span>
              {!many && photo.color === c && <Check className="ml-auto size-3.5" />}
            </ContextMenuItem>
          ))}
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => s.setColor("none", targets)}>None</ContextMenuItem>
        </ContextMenuSubContent>
      </ContextMenuSub>
      <ContextMenuItem onSelect={() => s.clearRatingAndColor(targets)}>Clear Stars and Colour</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void api.reveal(photo.path).catch(logged("revealing a photo"))}>Reveal in {fileBrowser}</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void s.transfer(targets, false)}>Copy {subject}…</ContextMenuItem>
      <ContextMenuItem onSelect={() => void s.transfer(targets, true)}>Move {subject}…</ContextMenuItem>
      <ContextMenuItem onSelect={() => s.openShip(targets)}>Ship {subject}</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem variant="destructive" onSelect={() => s.requestTrash(targets)}>
        {many ? `Move ${targets.length} Photos to Trash` : "Move to Trash"}
      </ContextMenuItem>
    </>
  );
}
