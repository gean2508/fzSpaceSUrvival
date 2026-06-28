import type { ZodError } from "zod";

/** Flatten zod failure for server logs — never discard invalid WS messages silently. */
export function formatZodFailure(error: ZodError): {
  issues: { path: string; code: string; message: string }[];
} {
  return {
    issues: error.issues.map((i) => ({
      path: i.path.join(".") || "(root)",
      code: i.code,
      message: i.message,
    })),
  };
}
