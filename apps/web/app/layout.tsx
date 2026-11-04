import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "./providers";
import { ServiceWorkerRegistration } from "./service-worker-registration";
import { WasmPackage } from "./wasm-package";
import { Geist } from "next/font/google";
import { cn } from "@/lib/utils";

const geist = Geist({ subsets: ["latin"], variable: "--font-sans" });

export const metadata: Metadata = {
  title: "Block World Renderer",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="ja"
      suppressHydrationWarning
      className={cn("font-sans", geist.variable)}
    >
      <body>
        <Providers>
          <ServiceWorkerRegistration />
          <WasmPackage />
          {children}
        </Providers>
      </body>
    </html>
  );
}
