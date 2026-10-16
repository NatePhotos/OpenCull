import { useEffect, useState } from "react";
import { AlertTriangle, X } from "lucide-react";
import { api, type AiAccelerationInfo } from "@/lib/api";
import { accelerationError } from "@/lib/ai-acceleration-error";

/** A worker failure stays visible until dismissed; polling also catches failures
 * after Settings opens. Unsupported platforms return null and show no CUDA UI. */
export function AiAccelerationError() {
  const [info, setInfo] = useState<AiAccelerationInfo | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const read = () => { void api.aiAccelerationInfo().then(value => { if (live) setInfo(value); }).catch(() => {}); };
    read(); const timer = window.setInterval(read, 2000);
    return () => { live = false; window.clearInterval(timer); };
  }, []);
  const error = accelerationError(info);
  if (!error || dismissed === error.details) return null;
  return <div role="alert" className="flex shrink-0 items-start gap-3 border border-red-500/60 bg-red-950 px-4 py-3 text-sm text-red-50">
    <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-red-300" />
    <div className="min-w-0 flex-1"><p className="font-semibold">{error.title}</p><p className="mt-1">{error.body}</p><details className="mt-2"><summary className="cursor-pointer text-red-200">Technical details</summary><p className="mt-1 break-words text-xs">{error.details}</p></details></div>
    <button type="button" aria-label="Dismiss GPU error" className="rounded p-1 hover:bg-red-900 focus-visible:outline-2 focus-visible:outline-offset-2" onClick={() => setDismissed(error.details)}><X aria-hidden="true" className="size-4" /></button>
  </div>;
}
