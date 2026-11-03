"use client";

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import { UserFacingError } from "@/lib/errors";
import { hasGraphicsApi, startWorld, type LoadProgress } from "@/lib/renderer";

const subscribeToNothing = () => () => {};

export function WorldCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { t } = useTranslation();
  const [progress, setProgress] = useState<LoadProgress | null>(null);
  const supported = useSyncExternalStore(
    subscribeToNothing,
    hasGraphicsApi,
    () => true,
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !supported) return;

    let cancelled = false;
    let stop: (() => void) | undefined;
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
        setProgress(null);
        throw new UserFacingError(t("errors.startFailed"), error);
      });

    const release = () => {
      cancelled = true;
      stop?.();
      stop = undefined;
    };
    window.addEventListener("pagehide", release);

    return () => {
      window.removeEventListener("pagehide", release);
      release();
    };
  }, [supported, t]);

  return (
    <>
      <canvas ref={canvasRef} className="world" />
      {!supported && <p className="notice">{t("graphicsUnsupported")}</p>}
      {progress && (
        <div className="fixed inset-x-0 bottom-0 p-4">
          {progress.available && progress.status && (
            <p className="mb-2 font-[system-ui] text-sm text-foreground">
              {progress.status}
            </p>
          )}
          <Progress
            value={progress.available ? progress.fraction * 100 : null}
          />
        </div>
      )}
    </>
  );
}
