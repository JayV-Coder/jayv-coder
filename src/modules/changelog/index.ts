import { getVersion } from "@tauri-apps/api/app";
import { create } from "zustand";
import { RELEASES, type Release } from "./releases";

export * from "./releases";

/** A última versão cujas novidades a pessoa já viu, neste aparelho. */
const SEEN_KEY = "jayv.whatsNew.seen";

/** Compara `MAJOR.MINOR.PATCH`; o que vier depois do `-` não conta. */
export function compareVersions(a: string, b: string) {
  const parts = (version: string) => version.split("-")[0].split(".").map((part) => Number.parseInt(part, 10) || 0);
  const [left, right] = [parts(a), parts(b)];
  for (let index = 0; index < 3; index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0);
    if (difference !== 0) return Math.sign(difference);
  }
  return 0;
}

/** As versões até a instalada (as de um build mais novo que o app não
 * aparecem), da mais nova para a mais antiga. */
export function releasesUpTo(current: string, releases: Release[] = RELEASES) {
  return releases.filter((release) => compareVersions(release.version, current) <= 0);
}

/** O que a pessoa ainda não viu: as versões depois de `seen` até a
 * instalada. Sem `seen` (a primeira abertura com esta janela, ou uma
 * instalação nova), só a versão instalada, para não despejar o histórico. */
export function unseenReleases(current: string, seen: string | null, releases: Release[] = RELEASES) {
  const installed = releasesUpTo(current, releases);
  if (!seen) return installed.filter((release) => release.version === current);
  return installed.filter((release) => compareVersions(release.version, seen) > 0);
}

/** As novidades que o release leva nas notas (`.github/scripts/whats-new.cjs`):
 * um comentário com JSON em base64. Sem ele, ou com ele quebrado, nada. */
export function releaseFromNotes(notes: string | null | undefined): Release | null {
  const encoded = notes?.match(/<!-- whats-new: ([A-Za-z0-9+/=]+) -->/)?.[1];
  if (!encoded) return null;
  try {
    const bytes = Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0));
    const parsed = JSON.parse(new TextDecoder().decode(bytes)) as Partial<Release>;
    if (typeof parsed.version !== "string" || typeof parsed.date !== "string" || !Array.isArray(parsed.items)) return null;
    const items = parsed.items.filter((item) => (item?.kind === "feature" || item?.kind === "fix") && typeof item.id === "string");
    return { version: parsed.version, date: parsed.date, items };
  } catch {
    return null;
  }
}

interface ChangelogState {
  open: boolean;
  /** A versão instalada; nula até o núcleo dizer. */
  current: string | null;
  /** De que versão a pessoa veio, quando a janela abriu sozinha. */
  previous: string | null;
  /** As versões que a janela mostra de cara. */
  releases: Release[];
  /** Se o histórico inteiro está à mostra. */
  all: boolean;
  /** A versão nova que ainda não foi instalada, quando a janela mostra o que
   * ela traz (pelo "O que muda" do aviso de atualização). */
  upcoming: string | null;
}

export const useChangelog = create<ChangelogState>(() => ({ open: false, current: null, previous: null, releases: [], all: false, upcoming: null }));

function readSeen() {
  try {
    return localStorage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
}

function writeSeen(version: string) {
  try {
    localStorage.setItem(SEEN_KEY, version);
  } catch {
    // Sem armazenamento a janela volta na próxima abertura; nada quebra.
  }
}

async function installedVersion() {
  const known = useChangelog.getState().current;
  if (known) return known;
  // Fora do Tauri (prévia do Vite) vale a entrada mais nova do changelog.
  const version = await getVersion().catch(() => RELEASES[0]?.version ?? "0.0.0");
  useChangelog.setState({ current: version });
  return version;
}

/** Depois de uma atualização, abre a janela com o que mudou desde a última
 * versão vista neste aparelho. Chamado quando a pessoa entra no app; a mesma
 * versão não abre duas vezes. */
export async function announceChanges() {
  const current = await installedVersion();
  const seen = readSeen();
  if (seen && compareVersions(current, seen) <= 0) return;
  const releases = unseenReleases(current, seen);
  if (!releases.length) { writeSeen(current); return; }
  useChangelog.setState({ open: true, previous: seen, releases, all: false, upcoming: null });
}

/** Abre a janela pelo botão: a versão instalada e o histórico inteiro. */
export async function showChanges() {
  const current = await installedVersion();
  useChangelog.setState({ open: true, previous: null, releases: releasesUpTo(current), all: true, upcoming: null });
}

/** Abre a janela só com a versão nova que o aviso de atualização oferece. */
export async function showUpcoming(release: Release) {
  await installedVersion();
  useChangelog.setState({ open: true, previous: null, releases: [release], all: true, upcoming: release.version });
}

/** Mostra também as versões mais antigas. */
export function showAllChanges() {
  const current = useChangelog.getState().current;
  if (current) useChangelog.setState({ releases: releasesUpTo(current), all: true, upcoming: null });
}

/** Fecha e marca a versão instalada como vista. Ver o que a próxima traz não
 * conta como ter visto: ela abre de novo depois de instalada. */
export function closeChanges() {
  const { current, upcoming } = useChangelog.getState();
  if (current && !upcoming) writeSeen(current);
  useChangelog.setState({ open: false });
}
