import { openUrl } from "@tauri-apps/plugin-opener";

/** O endereço público do site (repositório JayV-Coder/site), sem barra no
 * fim, vindo do build (`VITE_SITE_URL`). Criar organização, convidar membros
 * e a política de LLM moram no painel dele. Vazio, o app só avisa onde fica. */
export const SITE_URL = (import.meta.env.VITE_SITE_URL ?? "").trim().replace(/\/+$/, "");

/** O caminho do painel no site; o site escolhe o idioma pelo cookie ou pelo
 * navegador. */
export const dashboardUrl = (path = "") => `${SITE_URL}/dashboard${path}`;

export async function openDashboard(path = "") {
  if (!SITE_URL) return;
  await openUrl(dashboardUrl(path));
}

/** Abre uma página pública do site (a documentação, as versões); sem o
 * endereço do site no build, não faz nada. */
export async function openSite(path = "") {
  if (!SITE_URL) return;
  await openUrl(`${SITE_URL}${path}`);
}
