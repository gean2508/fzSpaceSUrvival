/**
 * Timestamped server logs — observability rule 9.
 */
export function slog(tag: string, message: string, data?: Record<string, unknown>) {
  const ts = new Date().toISOString();
  const extra = data ? ` ${JSON.stringify(data)}` : "";
  console.log(`[${ts}] [${tag}] ${message}${extra}`);
}
