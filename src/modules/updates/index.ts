import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { toast } from "sonner";
import { notify } from "@/modules/feedback";
import { t } from "@/modules/i18n";

/** Pergunta ao repositório de releases se há versão nova. O aviso fica na tela
 * até a pessoa decidir: atualizar baixa, instala e reabre o aplicativo. Sem
 * rede, ou numa build de desenvolvimento, a consulta falha calada — não há o
 * que avisar a quem só quer trabalhar. */
export async function checkForUpdate() {
  let update: Update | null;
  try {
    update = await check();
  } catch (error) {
    console.warn("update check failed", error);
    return;
  }
  if (!update) return;
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
