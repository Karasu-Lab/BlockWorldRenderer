import i18n from "@/lib/i18n";

export class UserFacingError extends Error {
  constructor(message: string, cause?: unknown) {
    super(message, { cause });
    this.name = "UserFacingError";
  }
}

export class TranslatedError extends Error {
  constructor(
    readonly key: string,
    readonly params: Record<string, string | number> = {},
  ) {
    super(key);
    this.name = "TranslatedError";
  }
}

export function describe(error: unknown): string {
  if (error instanceof TranslatedError) {
    return i18n.t(error.key, error.params);
  }
  return error instanceof Error ? error.message : String(error);
}
