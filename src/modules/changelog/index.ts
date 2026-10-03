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
}

export const useChangelog = create<ChangelogState>(() => ({ open: false, current: null, previous: null, releases: [], all: false }));

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
  useChangelog.setState({ open: true, previous: seen, releases, all: false });
}

/** Abre a janela pelo botão: a versão instalada e o histórico inteiro. */
export async function showChanges() {
  const current = await installedVersion();
  useChangelog.setState({ open: true, previous: null, releases: releasesUpTo(current), all: true });
}

/** Mostra também as versões mais antigas. */
export function showAllChanges() {
  const current = useChangelog.getState().current;
  if (current) useChangelog.setState({ releases: releasesUpTo(current), all: true });
}

/** Fecha e marca a versão instalada como vista. */
export function closeChanges() {
  const current = useChangelog.getState().current;
  if (current) writeSeen(current);
  useChangelog.setState({ open: false });
}
