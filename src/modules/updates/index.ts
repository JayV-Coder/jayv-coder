import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { toast } from "sonner";
import { notify } from "@/modules/feedback";
import { t } from "@/modules/i18n";

/** Pergunta ao repositório de releases se há versão nova. O aviso fica na tela
 * até a pessoa decidir: atualizar baixa, instala e reabre o aplicativo. Na
 * consulta automática, sem rede ou numa build de desenvolvimento, a falha fica
 * calada — não há o que avisar a quem só quer trabalhar. Quem pediu pelo botão
 * (`announce`) ouve também "já está na mais nova" e o erro. */
export async function checkForUpdate(announce = false) {
  let update: Update | null;
  try {
    update = await check();
  } catch (error) {
    console.warn("update check failed", error);
    if (announce) notify(t("update.checkFailed", { error: String(error) }), true);
    return;
  }
  if (!update) {
    if (announce) notify(t("update.latest"));
    return;
  }
  const found = update;
  toast(t("update.available", { version: found.version }), {
    duration: Infinity,
    action: { label: t("update.install"), onClick: () => void install(found) },
  });
}

async function install(update: Update) {
  const progress = toast.loading(t("update.downloading", { version: update.version }));
  try {
    await update.downloadAndInstall();
    await relaunch();
  } catch (error) {
    toast.dismiss(progress);
    notify(t("update.failed", { error: String(error) }), true);
  }
}
