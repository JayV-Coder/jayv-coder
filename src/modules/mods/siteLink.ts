import { commands } from "@/modules/core";
import { useEnvironment } from "@/modules/environments";
import { notify, reportError } from "@/modules/feedback";
import { t, useI18n } from "@/modules/i18n";
import { openSettingsTab } from "@/modules/settings/tab";
import { awaitSiteInstall, findMod, modText, setInstalledTab, useMods } from "./index";

/** Instalar um mod é no site: na página dele, com a conta aberta lá, o botão
 * Instalar no JayV grava o mod no estado dos mods do ambiente escolhido (o
 * mesmo `installedMods` que o app sincroniza) e abre
 * `jayv://mods/sync?mod=<id>&environment=<ambiente>`. O link não instala nada
 * nem diz o que instalar: ele só acorda o sync, que desce o estado gravado na
 * conta e instala o que ele pede. Um link falso, então, no máximo sincroniza
 * mais cedo. */

const MOD_ID = /^[a-z][A-Za-z0-9-]{1,39}$/;
const ENVIRONMENT = /^(personal|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/;

export interface ModsLink { mod: string | null; environment: string | null }

/** O link `jayv://mods/sync`, com o mod e o ambiente só quando têm o formato
 * certo; nulo para qualquer outro link. */
export function readModsLink(url: string): ModsLink | null {
  let parsed: URL;
  try { parsed = new URL(url); } catch { return null; }
  if (parsed.protocol !== "jayv:" || parsed.host !== "mods" || parsed.pathname.replace(/^\/+|\/+$/g, "") !== "sync") return null;
  const mod = parsed.searchParams.get("mod");
  const environment = parsed.searchParams.get("environment");
  return { mod: mod && MOD_ID.test(mod) ? mod : null, environment: environment && ENVIRONMENT.test(environment) ? environment : null };
}

/** Atende o link do site: sincroniza agora e abre Configuração › Mods ›
 * Instalados, onde o mod aparece quando descer (com um aviso). Instalado em
 * outro ambiente, só avisa: este app não o recebe. `true` quando o link era
 * dele. */
export function receiveModsLink(url: string): boolean {
  const link = readModsLink(url);
  if (!link) return false;
  const name = () => {
    const manifest = link.mod ? findMod(useMods.getState().catalog, link.mod) : undefined;
    return manifest ? modText(manifest.title, useI18n.getState().locale) : link.mod ?? "";
  };
  if (link.environment && link.environment !== useEnvironment.getState().active) {
    notify(t("mods.fromSite.otherEnvironment", { name: name() }));
    return true;
  }
  setInstalledTab("installed");
  openSettingsTab("mods");
  if (link.mod) awaitSiteInstall(link.mod);
  void commands.syncNow().catch(reportError);
  return true;
}
