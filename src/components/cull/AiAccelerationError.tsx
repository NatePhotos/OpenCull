import { useEffect, useState } from "react";
import { AlertTriangle, X } from "lucide-react";
import { api } from "@/lib/api";
import { accelerationError } from "@/lib/ai-acceleration-error";

/** Keep a CUDA worker failure visible; unsupported platforms report no error. */
export function AiAccelerationError() {
  const [info, setInfo] = useState<Awaited<ReturnType<typeof api.aiAccelerationInfo>>>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    // Machines with no CUDA path answer null: ask once and stop asking.
    const read = () => {
      void api
        .aiAccelerationInfo()
        .then((value) => {
          if (!live) return;
          setInfo(value);
          if (value === null) window.clearInterval(timer);
        })
        .catch(() => {});
    };
    const timer = window.setInterval(read, 2000);
    read();
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, []);

  const error = accelerationError(info);
  if (!error || dismissed === error.details) return null;

  return (
    <div
      role="alert"
      className="flex shrink-0 items-start gap-3 border border-red-500/60 bg-red-950 px-4 py-3 text-sm text-red-50"
    >
      <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-red-300" />
      <div className="min-w-0 flex-1">
        <p className="font-semibold">{error.title}</p>
        <p className="mt-1">{error.body}</p>
        <details className="mt-2">
          <summary className="cursor-pointer text-red-200">Technical details</summary>
          <p className="mt-1 break-words text-xs">{error.details}</p>
        </details>
      </div>
      <button
        type="button"
        aria-label="Dismiss GPU error"
        className="rounded p-1 hover:bg-red-900 focus-visible:outline-2 focus-visible:outline-offset-2"
        onClick={() => setDismissed(error.details)}
      >
        <X aria-hidden="true" className="size-4" />
      </button>
    </div>
  );
}
