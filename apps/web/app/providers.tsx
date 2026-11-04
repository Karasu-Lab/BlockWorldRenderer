"use client";

import { useEffect, type ReactNode } from "react";
import { ThemeProvider } from "next-themes";
import { I18nextProvider, useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Toaster } from "@/components/ui/sonner";
import { UserFacingError, describe } from "@/lib/errors";
import { DownloaderProvider } from "@/lib/downloader";
import i18n from "@/lib/i18n";
import { ReplayProvider } from "@/lib/replay";
import { WorldProvider } from "@/lib/world";

function ErrorBody({ title, detail }: { title: string; detail: string }) {
  const { t } = useTranslation();

  return (
    <div
      className="flex cursor-text select-text flex-col gap-1"
      onPointerDown={(event) => event.stopPropagation()}
    >
      <p className="font-medium">{title}</p>
      <p className="text-xs break-all whitespace-pre-wrap opacity-80">
        {detail}
      </p>
      <button
        type="button"
        className="cursor-pointer self-start text-xs underline"
        onClick={() => void navigator.clipboard.writeText(`${title}\n${detail}`)}
      >
        {t("errors.copy")}
      </button>
    </div>
  );
}

function GlobalErrorToasts() {
  const { t } = useTranslation();

  useEffect(() => {
    const report = (error: unknown) => {
      console.error(error);
      const [title, detail] =
        error instanceof UserFacingError
          ? [error.message, describe(error.cause)]
          : [t("errors.unexpected"), describe(error)];
      toast.error(<ErrorBody title={title} detail={detail} />);
    };
    const onError = (event: ErrorEvent) => report(event.error ?? event.message);
    const onRejection = (event: PromiseRejectionEvent) => report(event.reason);

    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, [t]);

  return null;
}

export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
    >
      <I18nextProvider i18n={i18n}>
        <GlobalErrorToasts />
        <DownloaderProvider>
          <WorldProvider>
            <ReplayProvider>{children}</ReplayProvider>
          </WorldProvider>
        </DownloaderProvider>
        <Toaster />
      </I18nextProvider>
    </ThemeProvider>
  );
}
