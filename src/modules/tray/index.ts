import { commands, onCore } from "@/modules/core";
import { t, useI18n } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { installUpdate, showUpdate, useUpdate } from "@/modules/updates";
import { openStats } from "@/modules/usage";
import { createChat, leaveProject, useWorkspace } from "@/modules/workspace";

/** O que cada item do menu da bandeja faz, depois que o núcleo já trouxe a
 * janela de volta. "Novo chat" sem projeto aberto leva à lista de projetos. */
export function runTrayAction(action: string) {
  switch (action) {
    case "newChat": {
      const projectId = useWorkspace.getState().activeProjectId;
      if (projectId) void createChat(projectId);
      else leaveProject();
      break;
    }
    case "projects": leaveProject(); break;
    case "organizations": navigate("organizations"); break;
    case "stats": openStats({ kind: "global" }); break;
    case "system": navigate("status"); break;
    case "settings": navigate("settings"); break;
    case "update":
      if (useUpdate.getState().phase === "available") void installUpdate();
      else showUpdate();
      break;
  }
}

/** O menu no idioma da tela, com a versão nova no item de atualizar quando
 * ela já foi achada. */
function sendLabels() {
  const { phase, next } = useUpdate.getState();
  const ready = phase === "available" && next;
  void commands.setTrayLabels({
    open: t("tray.open"),
    newChat: t("common.newChat"),
    projects: t("nav.projects"),
    organizations: t("nav.organizations"),
    stats: t("nav.stats"),
    system: t("nav.system"),
    settings: t("nav.settings"),
    update: ready ? t("tray.updateTo", { version: next }) : t("system.update.check"),
    quit: t("tray.quit"),
    tooltip: ready ? t("tray.tooltip.update", { version: next }) : "JayV",
  }).catch(() => {});
}

/** Liga o menu da bandeja: os cliques viram ações na tela, e o texto acompanha
 * o idioma e a versão nova. */
export function connectTray() {
  const off = onCore("tray-action", ({ action }) => runTrayAction(action));
  sendLabels();
  const offI18n = useI18n.subscribe((state, previous) => {
    if (state.locale !== previous.locale || state.messages !== previous.messages) sendLabels();
  });
  const offUpdate = useUpdate.subscribe((state, previous) => {
    if (state.phase !== previous.phase || state.next !== previous.next) sendLabels();
  });
  return () => {
    void off.then((unlisten) => unlisten());
    offI18n();
    offUpdate();
  };
}
