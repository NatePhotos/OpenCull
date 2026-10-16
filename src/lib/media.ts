const VIDEO_EXTENSIONS = new Set(["mp4", "mov", "m4v", "avi", "mkv", "webm", "mts", "m2ts", "mpg", "mpeg", "3gp"]);
export function isVideo(path: string): boolean {
  return VIDEO_EXTENSIONS.has(path.split(/[\\/]/).pop()?.split(".").pop()?.toLowerCase() ?? "");
}
