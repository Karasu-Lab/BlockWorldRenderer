import { TranslatedError } from "@/lib/errors";

export async function fetchBytes(
  url: string,
  onBytes?: (received: number, total: number) => void,
): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new TranslatedError("errors.downloadFailed", {
      url,
      status: response.status,
    });
  }
  const total = Number(response.headers.get("content-length")) || 0;
  if (!onBytes || !response.body || total === 0) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    onBytes?.(bytes.length, bytes.length);
    return bytes;
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.length;
    onBytes(received, total);
  }
  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

export async function digest(
  algorithm: "SHA-1" | "SHA-256",
  bytes: Uint8Array,
): Promise<string> {
  const hash = await crypto.subtle.digest(algorithm, bytes as BufferSource);
  return [...new Uint8Array(hash)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
