"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Progress } from "@/components/ui/progress";
import { hasWebGpu, startWorld, type LoadProgress } from "@/lib/renderer";

const subscribeToNothing = () => () => {};

export function WorldCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [progress, setProgress] = useState<LoadProgress | null>(null);
  // The server cannot tell, so it renders as if WebGPU were there.
  const supported = useSyncExternalStore(subscribeToNothing, hasWebGpu, () => true);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !supported) return;

    let cancelled = false;
    let stop: (() => void) | undefined;
    // Indeterminate until the wasm side confirms the renderer is
    // available; the download's own progress counts from there.
    setProgress({ available: false, fraction: 0, status: "" });
    startWorld(canvas, (next) => {
      if (!cancelled) setProgress(next);
    })
      .then((world) => {
        setProgress(null);
        if (cancelled) {
          world.stop();
          world.free();
          return;
        }
        stop = () => {
          world.stop();
          world.free();
        };
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        console.error(error);
        setProgress(null);
        setFailure("ワールドの描画を開始できませんでした。");
      });

    return () => {
      cancelled = true;
      stop?.();
    };
  }, [supported]);

  return (
    <>
      <canvas ref={canvasRef} className="world" />
      {!supported && (
        <p className="notice">このブラウザは WebGPU に対応していません。</p>
      )}
      {failure && <p className="notice">{failure}</p>}
      {progress && (
        <div className="fixed inset-x-0 bottom-0 p-4">
          {progress.available && progress.status && (
            <p className="mb-2 font-[system-ui] text-sm text-neutral-200">
              {progress.status}
            </p>
          )}
          <Progress value={progress.available ? progress.fraction * 100 : null} />
        </div>
      )}
    </>
  );
}
