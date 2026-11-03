import type { Metadata } from "next";
import "./globals.css";
import { ServiceWorkerRegistration } from "./service-worker-registration";

export const metadata: Metadata = {
  title: "Block World Renderer",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="ja">
      <body>
        <ServiceWorkerRegistration />
        {children}
      </body>
    </html>
  );
}
