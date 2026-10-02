import { authFailure } from "./errors";

/** `reason` é o `error_code` do Supabase, para quem precisa decidir pelo
 * código e não só mostrar o erro. */
export type Callback = { code: string } | { failure: unknown; reason: string | null };

/** O link `jayv://auth/...` que volta do navegador: o código do PKCE ou o erro.
 * O Supabase põe o erro na query ou no fragmento, conforme o fluxo. */
export function readCallback(url: string): Callback | null {
  let parsed: URL;
  try { parsed = new URL(url); } catch { return null; }
  if (parsed.protocol !== "jayv:" || parsed.host !== "auth") return null;
  const params = new URLSearchParams(parsed.search);
  new URLSearchParams(parsed.hash.replace(/^#/, "")).forEach((value, name) => { if (!params.has(name)) params.set(name, value); });
  const description = params.get("error_description");
  if (description || params.get("error")) {
    const reason = params.get("error_code");
    return { failure: authFailure({ code: reason ?? undefined, message: description ?? params.get("error") }), reason };
  }
  const code = params.get("code");
  return code ? { code } : null;
}
