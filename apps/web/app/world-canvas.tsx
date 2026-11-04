"use client";

import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import { useDownloadProgress } from "@/lib/downloader";
import { hasGraphicsApi } from "@/lib/renderer";
import { useWorld } from "@/lib/world";

const subscribeToNothing = () => () => {};

export function WorldCanvas() {
  const world = useWorld();
  const { t } = useTranslation();
  const supported = useSyncExternalStore(
    subscribeToNothing,
    hasGraphicsApi,
    () => true,
  );
  const progress = useDownloadProgress();
  const megabytes = (bytes: number) => (bytes / 1048576).toFixed(1);
  const status = progress.step
    ? t(`download.${progress.step}`, {
        file: progress.file,
        done:
          progress.step === "assets"
            ? megabytes(progress.done)
            : progress.done,
        total:
          progress.step === "assets"
            ? megabytes(progress.total)
            : progress.total,
      })
    : "";

  return (
    <>
      <canvas
        ref={supported ? world.attachCanvas : undefined}
        className="world"
      />
      {!supported && <p className="notice">{t("graphicsUnsupported")}</p>}
      {(world.isLoading || progress.active) && (
        <div className="fixed inset-x-0 bottom-0 p-4">
          <p className="mb-2 min-h-5 font-[system-ui] text-sm text-foreground">
            {status}
          </p>
          <Progress value={progress.fraction * 100} />
          <Progress value={progress.fileFraction * 100} className="mt-2" />
          {progress.wasmVersion && (
            <p className="mt-2 text-left font-[system-ui] text-sm text-foreground">
              {progress.wasmVersion}
            </p>
          )}
        </div>
      )}
    </>
  );
}
