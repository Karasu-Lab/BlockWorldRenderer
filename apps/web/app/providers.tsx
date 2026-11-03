"use client";

import { useEffect, type ReactNode } from "react";
import { ThemeProvider } from "next-themes";
import { I18nextProvider, useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Toaster } from "@/components/ui/sonner";
import { UserFacingError, describe } from "@/lib/errors";
import i18n from "@/lib/i18n";

function GlobalErrorToasts() {
  const { t } = useTranslation();

  useEffect(() => {
    const report = (error: unknown) => {
      console.error(error);
      if (error instanceof UserFacingError) {
        toast.error(error.message, { description: describe(error.cause) });
      } else {
        toast.error(t("errors.unexpected"), { description: describe(error) });
      }
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
        {children}
        <Toaster />
      </I18nextProvider>
    </ThemeProvider>
  );
}
