export type Flag = "none" | "pick" | "reject";
export type Color = "none" | "red" | "yellow" | "green" | "blue" | "purple";

export const COLORS: Exclude<Color, "none">[] = ["red", "yellow", "green", "blue", "purple"];

/** What a photographer has decided about one photo. */
export interface Marks {
  rating: number;
  flag: Flag;
  color: Color;
}

export interface Exif {
  make: string | null;
  model: string | null;
  lens: string | null;
  focalLength: number | null;
  focalLength35mm: number | null;
  aperture: number | null;
  shutter: number | null;
  iso: number | null;
  exposureComp: number | null;
  /** `YYYY:MM:DD HH:MM:SS`, exactly as the camera wrote it. */
  date: string | null;
  /** Milliseconds on the camera's own clock. Only for comparing two frames. */
  captureMs: number | null;
  width: number | null;
  height: number | null;
  latitude: number | null;
  longitude: number | null;
}

/** One photo as the folder scan found it. */
export interface Scanned {
  /** The file that owns the photo's marks and sidecar. */
  path: string;
  /** The file drawn for it: the JPEG, for a RAW with one beside it. */
  display: string;
  name: string;
  subpath: string | null;
  /** Changes when the drawn file does; part of every preview's address. */
  version: string;
  size: number;
  modified: number;
}

/**
 * A crop: a rectangle by its middle and its size, in pixels of the full-size
 * picture as it is viewed, turned `angle` degrees clockwise about its middle.
 */
export interface Crop {
  cx: number;
  cy: number;
  w: number;
  h: number;
  angle: number;
}

/**
 * How a photo is framed. Nothing is done to the picture: the previews are the
 * whole upright frame, and this says how to place them.
 */
export interface Framing {
  /**
   * How the upright picture is turned to give the view: one of the eight ways
   * a rectangle can be turned and mirrored, numbered as EXIF numbers them.
   * 1 is not at all.
   */
  turn: number;
  crop: Crop | null;
}

export interface Meta {
  index: number;
  marks: Marks;
  exif: Exif;
  fullWidth: number;
  fullHeight: number;
  framing: Framing;
  framable: boolean;
}

export type MetaEvent =
  | { type: "batch"; items: Meta[] }
  | { type: "done"; groups: (number | null)[] };

export interface Photo extends Scanned, Marks {
  /** Position in the folder scan. Stable for as long as the folder is open. */
  index: number;
  exif: Exif | null;
  /**
   * When the photo was taken, on the camera's clock. Known as the folder
   * opens, ahead of the rest of the EXIF, because the folder's order hangs on it.
   */
  captureMs: number | null;
  /** The size of the upright picture, before any turn or crop of the photographer's. */
  fullWidth: number;
  fullHeight: number;
  /** How it is turned and cropped. */
  framing: Framing;
  /** Whether it can be turned and cropped here at all. */
  framable: boolean;
  /** Which burst this frame belongs to, or null if it stands alone. */
  group: number | null;
}

/** One face found in a photo. Its box is in fractions of the upright photo. */
export interface AiFace {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Someone the photo is about, as against someone in the background. */
  subject: boolean;
  /** Why it was left out, for a face that is not a subject. */
  reason: string | null;
  eyesShut: boolean;
  lookingAway: boolean;
  /** Not called looking away, but worth a look: everyone else in the photo is looking and this one may not be. */
  maybeAway: boolean;
  /** How sharp its eyes are, out of 100; null when it could not be told. */
  sharpness: number | null;
  /** How open its eyes measure, about 0.3 when wide open. */
  eye: number;
  /** Whether that can be set beside the same person's eyes in another frame. */
  eyesRead: boolean;
  /** The state it is in, in words — "Looking Down", "Smiling · Eyes Open" — or null where it could not be said. */
  state: string | null;
  /** How much there is to worry about in that: 0 nothing, 1 worth a look, 2 probably a fault, 3 a fault. */
  concern: number;
  /** How far that state excuses eyes that are not open, 0 to 1: nought where it gives no reason for them. */
  excused: number;
  /** The face's state says the eyes are open; auto-cull reads it. */
  open?: boolean;
}

/** What the analysis found in one photo. */
export interface AiResult {
  faces: AiFace[];
  /** Out of 100, on its subjects; null for a photo with no faces. */
  score: number | null;
  /** The parts of the mark that are not about the eyes, each 0 to 1. */
  focus: number | null;
  gaze: number | null;
  /**
   * With no faces: the part of the photo it is about, which it was marked
   * on. Left, top, width and height, as fractions of the upright photo.
   */
  key: { x: number; y: number; w: number; h: number } | null;
}

/** An analysed photo with when it was taken, as it is sent to be compared. */
/** `upright`: taller than it is wide, the way up it is shown; null if its shape is not known. */
export type RunFrame = AiResult & { time: number | null; burst: number | null; upright: boolean | null; index?: number };

export type MobileClipGrouping = "loose" | "normal" | "tight";
export interface AiGroupingOptions {
  session?: number;
  revision?: number;
  mobileClip?: boolean;
  mobileClipGrouping?: MobileClipGrouping;
}
export interface MobileClipStatus {
  ready: boolean;
  bytes: number;
  model: string;
  backend: string;
}

/** A photo marked again as one frame of its burst, or of a pose shot again. */
export interface Compared {
  /**
   * Which pose of the run it was sent in. Frames with the same number were
   * compared with each other.
   */
  pose: number;
  /** Out of 100; null for a photo with no faces. */
  score: number | null;
  /**
   * For each face, in the photo's own order: how open its eyes are as a
   * share of the same person's best in the burst. Null where the person
   * could not be followed from frame to frame, or the eyes not read.
   */
  eyes: (number | null)[];
}

export type AiEvent = { type: "batch"; items: ({ index: number } & AiResult)[] } | { type: "done"; finished: boolean };

/** A photo's place among the frames it was compared with: 1 is the best of `count`. */
export interface Rank {
  rank: number;
  count: number;
}

export interface Histogram {
  r: number[];
  g: number[];
  b: number[];
  luma: number[];
}

/**
 * Which of a folder's files are its photos: a RAW and its JPEG together as
 * one, the RAWs alone, or everything that is not a RAW.
 */
export type FileKinds = "both" | "raw" | "jpeg";

export type ViewMode = "loupe" | "scene" | "grid";
export type SortKey = "captureTime" | "filename" | "rating";
export type FlagFilter = "all" | "picks" | "unflagged" | "rejects";

/** What kind of shoot a folder is; auto-cull judges each differently. */
export type ShootKind = "events" | "portraits" | "sports";
/** What a lesson is taught from: the stars given, or the picks and rejects flagged. */
export type LearnSource = "stars" | "flags";

/** One photo as auto-cull is told of it. */
export type AutocullFrame = AiResult & {
  path: string;
  version: string;
  comparedScore: number | null;
  comparedEyes: (number | null)[];
  rank: number | null;
  count: number | null;
  /** Frames sharing a number were compared with each other; null stands alone. */
  pose: number | null;
  rating: number;
  /** When it was taken, in milliseconds; null when unknown. */
  time: number | null;
  flag: Flag;
};
/** The new judging's verdict on one photo. */
export interface Judgement {
  path: string;
  color: Color;
  /** Its place in its burst, 1 for the best; null for a photo on its own. */
  place: number | null;
  count: number | null;
  /** An obvious reject: placed after every other frame of its burst. */
  reject: boolean;
  /** One line on why, for the info panel. */
  why: string;
}
/** A photo shot near the ones being judged, sent only for its light. */
export interface AutocullAround { path: string; version: string; time: number }
/** Why a retrained version was not kept, though there was enough to check it. */
export type AutocullReason = "worse" | "fewerRejects" | "chance";
export interface AutocullLearned {
  /** Shoots learned from, set-aside ones left out. */
  shoots: number;
  /** On bursts of the newest shoot each had not seen, 0 to 1, keepers, rejects and burst order counting equally. */
  before: number;
  after: number;
  accepted: boolean;
  enough: boolean;
  reason: AutocullReason | null;
  /** On those unseen bursts: how often the 5★ frame is the first choice, and in the top two. Null when none was checked. */
  pickFirst: number | null;
  pickTop2: number | null;
}
/** What became of the shoot's lessons: learned from, saved for later, set aside, or this folder's earlier lessons kept. */
export type AutocullKept = "learned" | "saved" | "setAside" | "earlier";
export interface AutocullReply {
  learned: AutocullLearned;
  /** How well the version now in use did on bursts of this shoot it had not seen. */
  matched: number;
  kept: AutocullKept;
  /** Which marks the lesson was taught from. */
  source: LearnSource;
}
export interface AutocullStatus { shoots: number; last: AutocullReply | null }
export interface AutocullOwned { version: string; color: Color; /** Why auto-cull gave it that colour; kept so it survives reopening the folder. */ reason?: string }
