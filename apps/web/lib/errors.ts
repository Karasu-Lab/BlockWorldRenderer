export class UserFacingError extends Error {
  constructor(message: string, cause?: unknown) {
    super(message, { cause });
    this.name = "UserFacingError";
  }
}

export function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
