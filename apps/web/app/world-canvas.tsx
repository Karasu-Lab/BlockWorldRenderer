"use client";

import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
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
  const progress = world.progress;

  return (
    <>
      <canvas
        ref={supported ? world.attachCanvas : undefined}
        className="world"
      />
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
          {progress.fileFraction !== null && (
            <Progress value={progress.fileFraction * 100} className="mt-2" />
          )}
        </div>
      )}
    </>
  );
}
