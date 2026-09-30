export function shorten(text: string | null | undefined, limit: number) {
  const clean = String(text ?? "").replace(/\s+/g, " ").trim();
  return clean.length > limit ? `${clean.slice(0, limit - 1)}…` : clean;
}
