"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
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
    startWorld(canvas, setProgress)
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
      {progress && progress.status && (
        <p className="notice">
          {progress.status} ({Math.round(progress.fraction * 100)}%)
        </p>
      )}
    </>
  );
}
