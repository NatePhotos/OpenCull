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
import { api } from "@/lib/api";
import { contextTargets, useStore } from "@/lib/store";
import { COLORS } from "@/lib/types";
import { isPlain } from "@/lib/framing";

/**
 * The right-click menu for a photo, wherever it is drawn.
 *
 * Acts on the whole selection when the photo clicked is part of it, and on
 * that one photo otherwise — the way a file browser's menu does.
 */
export function PhotoMenu({ index, children }: { index: number; children: ReactNode }) {
  const platform = useStore((s) => s.platform);
  const fileBrowser = platform === "macos" ? "Finder" : platform === "windows" ? "Explorer" : "File Manager";

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
  const framable = targets.some((i) => s.photos[i]?.framable);
  const framed = targets.some((i) => s.photos[i] && !isPlain(s.photos[i].framing));

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
      {framable ? (
        <>
          {!many && <ContextMenuItem onSelect={() => s.openCrop(index)}>Crop and Straighten…</ContextMenuItem>}
          <ContextMenuItem onSelect={() => s.turnPhotos("left", targets)}>Rotate Left</ContextMenuItem>
          <ContextMenuItem onSelect={() => s.turnPhotos("right", targets)}>Rotate Right</ContextMenuItem>
          <ContextMenuItem onSelect={() => s.turnPhotos("across", targets)}>Flip Horizontal</ContextMenuItem>
          <ContextMenuItem onSelect={() => s.turnPhotos("down", targets)}>Flip Vertical</ContextMenuItem>
          {!many && photo.framing.crop && <ContextMenuItem onSelect={() => s.copyCrop(index)}>Copy Crop</ContextMenuItem>}
          {s.copiedCrop && <ContextMenuItem onSelect={() => s.pasteCrop(targets)}>{many ? `Paste Crop onto ${targets.length} Photos` : "Paste Crop"}</ContextMenuItem>}
          {framed && <ContextMenuItem onSelect={() => s.resetFraming(targets)}>Reset Crop and Rotation</ContextMenuItem>}
        </>
      ) : (
        <ContextMenuItem disabled>{many ? "These photos" : "This photo"} cannot be cropped or rotated here</ContextMenuItem>
      )}
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void api.reveal(photo.path)}>Reveal in {fileBrowser}</ContextMenuItem>
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
