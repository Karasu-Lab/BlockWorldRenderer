import type { Metadata } from "next";
import "./globals.css";
import { ServiceWorkerRegistration } from "./service-worker-registration";
import { Geist } from "next/font/google";
import { cn } from "@/lib/utils";

const geist = Geist({subsets:['latin'],variable:'--font-sans'});

export const metadata: Metadata = {
  title: "Block World Renderer",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="ja" className={cn("font-sans", geist.variable)}>
      <body>
        <ServiceWorkerRegistration />
        {children}
      </body>
    </html>
  );
}
