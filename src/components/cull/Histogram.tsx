import type { ReactNode } from "react";

import { isVideo } from "@/lib/media";
import type { Photo } from "@/lib/types";

/** Keeps still-photo histogram hooks out of the video rendering path. */
export function HistogramGuard({ photo, children }: { photo: Photo | undefined; children: ReactNode }) {
  if (photo && isVideo(photo.path)) {
    return <p className="p-3 text-xs text-muted-foreground">Histogram unavailable for video</p>;
  }

  return <>{children}</>;
}
