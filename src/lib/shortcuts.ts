// Keyboard shortcuts: what can be bound, what each is bound to out of the
// box, and how a key press is matched to one.

import { useCrop } from "./crop";
import { QUARTER_LEFT, QUARTER_RIGHT } from "./framing";
import { load, save } from "./prefs";
import { useStore } from "./store";
import type { Color } from "./types";

export type Action =
  | "nextImage" | "previousImage" | "nextGroup" | "previousGroup"
  | "pick" | "reject" | "unflag"
  | "rate0" | "rate1" | "rate2" | "rate3" | "rate4" | "rate5" | "clearRating"
  | "tagRed" | "tagYellow" | "tagGreen" | "tagBlue" | "tagPurple" | "tagNone"
  | "zoomToggle" | "zoomCentre" | "zoomIn" | "zoomOut"
  | "viewGrid" | "toggleFilmstrip" | "toggleFaces" | "toggleHistogram" | "toggleInfo" | "toggleSpeedCull"
  | "ship" | "moveToTrash" | "selectAll" | "undo" | "redo"
  | "crop" | "rotateLeft" | "rotateRight" | "copyCrop" | "pasteCrop";

/** A key, by its position on the keyboard, and the modifiers held with it. */
export interface Combo {
  code: string;
  /** Command on a Mac, Control everywhere else. */
  mod?: boolean;
  alt?: boolean;
  shift?: boolean;
}

export const SECTIONS: { title: string; actions: [Action, string][] }[] = [
  {
    title: "Navigate",
    actions: [
      ["nextImage", "Next photo"],
      ["previousImage", "Previous photo"],
      ["nextGroup", "Next group"],
      ["previousGroup", "Previous group"],
    ],
  },
  {
    title: "Flag",
    actions: [
      ["pick", "Pick"],
      ["reject", "Reject"],
      ["unflag", "Unflag"],
    ],
  },
  {
    title: "Rate",
    actions: [
      ["rate1", "1 star"],
      ["rate2", "2 stars"],
      ["rate3", "3 stars"],
      ["rate4", "4 stars"],
      ["rate5", "5 stars"],
      ["rate0", "No stars"],
      ["clearRating", "Clear stars and colour"],
    ],
  },
  {
    title: "Colour",
    actions: [
      ["tagRed", "Red"],
      ["tagYellow", "Yellow"],
      ["tagGreen", "Green"],
      ["tagBlue", "Blue"],
      ["tagPurple", "Purple"],
      ["tagNone", "No colour"],
    ],
  },
  {
    title: "View",
    actions: [
      ["zoomToggle", "Zoom to centre"],
      ["zoomCentre", "Zoom to face"],
      ["zoomIn", "Zoom in"],
      ["zoomOut", "Zoom out"],
      ["viewGrid", "Grid view"],
      ["toggleFilmstrip", "Filmstrip"],
      ["toggleFaces", "Face panel"],
      ["toggleHistogram", "Histogram and EXIF"],
      ["toggleInfo", "Info overlay"],
      ["toggleSpeedCull", "Speed cull"],
    ],
  },
  {
    title: "Edit",
    actions: [
      ["selectAll", "Select all"],
      ["undo", "Undo"],
      ["redo", "Redo"],
      ["moveToTrash", "Move to trash"],
      ["ship", "Ship"],
    ],
  },
  {
    title: "Frame",
    actions: [
      ["crop", "Crop"],
      ["rotateLeft", "Rotate left"],
      ["rotateRight", "Rotate right"],
      ["copyCrop", "Copy crop"],
      ["pasteCrop", "Paste crop"],
    ],
  },
];

/**
 * Caps Lock is a switch, not a key that is pressed: speed cull bound to it is
 * on for as long as its light is, which is how Lightroom does it. Bound to
 * any other key, pressing the key turns speed cull on and off.
 */
export const CAPS_LOCK = "CapsLock";

/** The Mac app's defaults, key for key, and Space for the middle of the photo. */
export const DEFAULTS: Record<Action, Combo> = {
  nextImage: { code: "ArrowRight" },
  previousImage: { code: "ArrowLeft" },
  nextGroup: { code: "ArrowDown" },
  previousGroup: { code: "ArrowUp" },
  pick: { code: "KeyP" },
  reject: { code: "KeyX" },
  unflag: { code: "KeyU" },
  rate0: { code: "Digit0" },
  rate1: { code: "Digit1" },
  rate2: { code: "Digit2" },
  rate3: { code: "Digit3" },
  rate4: { code: "Digit4" },
  rate5: { code: "Digit5" },
  clearRating: { code: "Digit0", mod: true, alt: true },
  tagRed: { code: "Digit6" },
  tagYellow: { code: "Digit7" },
  tagGreen: { code: "Digit8" },
  tagBlue: { code: "Digit9" },
  tagPurple: { code: "KeyN" },
  tagNone: { code: "Digit0", mod: true },
  zoomToggle: { code: "KeyZ" },
  zoomCentre: { code: "Space" },
  zoomIn: { code: "Equal", mod: true },
  zoomOut: { code: "Minus", mod: true },
  viewGrid: { code: "KeyG" },
  toggleFilmstrip: { code: "KeyF" },
  toggleFaces: { code: "Period" },
  toggleHistogram: { code: "KeyH" },
  toggleInfo: { code: "KeyI" },
  toggleSpeedCull: { code: CAPS_LOCK },
  ship: { code: "KeyE", mod: true },
  moveToTrash: { code: "Backspace" },
  selectAll: { code: "KeyA", mod: true },
  undo: { code: "KeyZ", mod: true },
  redo: { code: "KeyZ", mod: true, shift: true },
  crop: { code: "KeyR" },
  rotateLeft: { code: "BracketLeft", mod: true },
  rotateRight: { code: "BracketRight", mod: true },
  copyCrop: { code: "KeyC", mod: true },
  pasteCrop: { code: "KeyV", mod: true },
};

export const isMac = /Mac/.test(navigator.userAgent);

export function loadBindings(): Record<Action, Combo> {
  // Defaults underneath whatever was saved, so an action added in a later
  // version arrives with its key rather than with none.
  return { ...DEFAULTS, ...load<Partial<Record<Action, Combo>>>("shortcuts", {}) };
}

export function saveBindings(bindings: Record<Action, Combo>): void {
  save("shortcuts", bindings);
}

export function sameCombo(a: Combo, b: Combo): boolean {
  return a.code === b.code && !!a.mod === !!b.mod && !!a.alt === !!b.alt && !!a.shift === !!b.shift;
}

export function comboOf(e: KeyboardEvent): Combo {
  return { code: e.code, mod: isMac ? e.metaKey : e.ctrlKey, alt: e.altKey, shift: e.shiftKey };
}

const KEY_NAMES: Record<string, string> = {
  ArrowRight: "→", ArrowLeft: "←", ArrowUp: "↑", ArrowDown: "↓",
  Backspace: "⌫", Delete: "⌦", Enter: "↩", Escape: "⎋", Tab: "⇥", Space: "Space", CapsLock: "⇪ Caps Lock",
  Equal: "+", Minus: "−", Period: ".", Comma: ",", Slash: "/", Backslash: "\\",
  BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'", Backquote: "`",
};

/** How a combination is written on a key cap. */
export function comboLabel(c: Combo): string {
  const key = KEY_NAMES[c.code] ?? c.code.replace(/^Key|^Digit|^Numpad/, "");
  const mods = isMac
    ? `${c.mod ? "⌘" : ""}${c.alt ? "⌥" : ""}${c.shift ? "⇧" : ""}`
    : `${c.mod ? "Ctrl+" : ""}${c.alt ? "Alt+" : ""}${c.shift ? "Shift+" : ""}`;
  return mods + key;
}

const COLOR_OF: Partial<Record<Action, Color>> = {
  tagRed: "red", tagYellow: "yellow", tagGreen: "green", tagBlue: "blue", tagPurple: "purple", tagNone: "none",
};

/** Does what a shortcut stands for. */
export function perform(action: Action): void {
  const s = useStore.getState();
  if (!s.session) return;

  switch (action) {
    case "nextImage": return s.next();
    case "previousImage": return s.previous();
    case "nextGroup": return s.nextGroup();
    case "previousGroup": return s.previousGroup();
    case "pick": return s.setFlag("pick");
    case "reject": return s.setFlag("reject");
    case "unflag": return s.setFlag("none");
    case "rate0": case "rate1": case "rate2": case "rate3": case "rate4": case "rate5":
      return s.setRating(Number(action.slice(4)));
    case "clearRating": return s.clearRatingAndColor();
    case "tagNone": {
      // Clearing a colour clears it from everything selected, as the menu does.
      const targets = s.selected.size > 0 ? [...s.selected] : s.current >= 0 ? [s.current] : [];
      return s.setColor("none", targets);
    }
    case "tagRed": case "tagYellow": case "tagGreen": case "tagBlue": case "tagPurple":
      return s.setColor(COLOR_OF[action]!);
    case "zoomToggle": return s.zoomCommand("centre");
    case "zoomCentre": return s.zoomCommand("face");
    case "zoomIn": return s.zoomCommand("in");
    case "zoomOut": return s.zoomCommand("out");
    case "viewGrid": return s.set("viewMode", s.viewMode === "grid" ? "loupe" : "grid");
    case "toggleFilmstrip": return s.toggle("showFilmstrip");
    case "toggleFaces": return s.toggle("showFaces");
    case "toggleHistogram": return s.toggle("showExif");
    case "toggleInfo": return s.toggle("showInfo");
    case "toggleSpeedCull": return s.set("speedCullAlways", !s.speedCullAlways);
    case "ship": return s.openShip();
    case "moveToTrash": return s.requestTrash();
    case "selectAll": return s.selectAll();
    case "undo": return s.undo();
    case "crop": return s.openCrop();
    case "rotateLeft": return s.turnPhotos("left");
    case "rotateRight": return s.turnPhotos("right");
    case "copyCrop": return s.copyCrop();
    case "pasteCrop": return s.pasteCrop();
    case "redo": return s.redo();
  }
}

/**
 * A key pressed while a photo is being cropped. The keys are Lightroom's:
 * Enter keeps the crop and Escape leaves the photo as it was; X swaps tall
 * for wide, A holds or frees the shape, O steps through the guides and
 * Shift+O turns them. Moving to another photo keeps the crop and carries on
 * cropping there. Undo and redo step back and forth through what has been
 * done to this crop, and no further. Everything else does nothing, rather
 * than what it does to a photo that is not being cropped.
 */
function cropKey(e: KeyboardEvent, table: Record<Action, Combo>) {
  const s = useStore.getState();
  const crop = useCrop.getState();
  const pressed = comboOf(e);
  const is = (action: Action) => sameCombo(table[action], pressed);
  const plain = !pressed.mod && !pressed.alt;
  e.preventDefault();
  // Held down, undo and redo run back and forth through the steps.
  if (is("undo")) return crop.undo();
  if (is("redo")) return crop.redo();
  if (e.repeat) return;

  if (e.code === "Enter" || e.code === "NumpadEnter" || is("crop")) return s.closeCrop(true);
  if (e.code === "Escape") return crop.lining ? crop.setLining(false) : s.closeCrop(false);
  for (const step of ["nextImage", "previousImage", "nextGroup", "previousGroup"] as const) {
    if (is(step)) {
      s.acceptCrop();
      return perform(step);
    }
  }
  if (is("copyCrop")) return s.copyCrop();
  if (is("pasteCrop")) return crop.paste(s.copiedCrop);
  if (is("rotateLeft")) return crop.turnBy(QUARTER_LEFT);
  if (is("rotateRight")) return crop.turnBy(QUARTER_RIGHT);
  if (pressed.mod && pressed.alt && e.code === "KeyR") return crop.reset();
  if (plain && e.code === "KeyX") return crop.swap();
  if (plain && e.code === "KeyA") return crop.toggleLock();
  if (plain && e.code === "KeyO") return pressed.shift ? crop.turnOverlay() : crop.cycleOverlay();
}

/**
 * Listens for key presses and runs whatever they are bound to.
 *
 * Returns a function that stops listening. `bindings` is read through a
 * getter so that a shortcut changed in settings takes effect at once.
 */
export function installShortcuts(bindings: () => Record<Action, Combo>, recording: () => boolean): () => void {
  const onKey = (e: KeyboardEvent) => {
    // Speed cull follows Caps Lock for as long as that is its key, so it is
    // on for as long as the light is.
    const caps = bindings().toggleSpeedCull.code === CAPS_LOCK && e.getModifierState("CapsLock");
    if (caps !== useStore.getState().speedCull) useStore.getState().set("speedCull", caps);
    if (e.code === CAPS_LOCK) return;

    if (e.type !== "keydown" || recording()) return;
    const s = useStore.getState();
    // A dialog is open, or someone is typing: keys belong to that.
    if (s.overviewOpen || s.shortcutsOpen || s.settingsOpen || s.shipOpen || s.ingestOpen || s.notice || s.trashPending || s.updateOpen) return;

    // Where every program on the system keeps its settings.
    if (e.code === "Comma" && (isMac ? e.metaKey : e.ctrlKey) && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      s.set("settingsOpen", true);
      return;
    }
    const target = e.target as HTMLElement | null;
    if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
    // A menu is open. Letters typed into one jump to the item starting with
    // them, and "P" must not both find "Pick" and pick the photo behind it.
    if (document.querySelector('[role="menu"]')) return;

    // A photo is being cropped. Every key is the crop's until it is done:
    // X swaps the shape here and must not reject the photo beneath, nor
    // Delete bin it.
    if (s.cropOpen) {
      cropKey(e, bindings());
      return;
    }

    const pressed = comboOf(e);
    const table = bindings();
    let action = (Object.keys(table) as Action[]).find((a) => sameCombo(table[a], pressed));
    // Both delete keys trash a photo while that shortcut is on its default,
    // since which one a keyboard has depends on the keyboard.
    if (!action && e.code === "Delete" && !pressed.mod && !pressed.alt && !pressed.shift && sameCombo(table.moveToTrash, DEFAULTS.moveToTrash)) {
      action = "moveToTrash";
    }
    if (!action) return;

    e.preventDefault();
    // Held down, these would flip between fit and zoomed many times a second.
    if (e.repeat && (action === "zoomToggle" || action === "zoomCentre")) return;
    perform(action);
  };
  window.addEventListener("keydown", onKey);
  window.addEventListener("keyup", onKey);
  return () => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("keyup", onKey);
  };
}
