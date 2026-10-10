import { useEffect, useState } from "react";
import type { McpServer } from "@/modules/core";
import { notify } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { closeDraft, confirmDraft, useMcp } from "@/modules/mcp";
import { useNavigation } from "@/modules/navigation";
import { openSettingsTab } from "@/modules/settings";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { McpServerForm } from "./McpServerForm";

/** A conferência do servidor MCP: o que veio do `/mcp` do chat, da colagem
 * na tela ou do botão de editar. O comando roda neste computador com o que o
 * agente pedir, então nada entra sem dois cliques: Aplicar põe o servidor na
 * lista de Configurações › MCP, marcado como não salvo, e só o Salvar das
 * configurações grava. Vindo do chat, a tela vai até lá para esse Salvar. */
export function McpDraftDialog() {
  const t = useT();
  const draft = useMcp((state) => state.draft);
  const [servers, setServers] = useState<McpServer[]>([]);
  const [saving, setSaving] = useState(false);
  useEffect(() => setServers(draft?.servers ?? []), [draft]);
  const apply = async () => {
    setSaving(true);
    try {
      if (!(await confirmDraft(servers)) || useNavigation.getState().view === "settings") return;
      openSettingsTab("mcp");
      notify(t("mcp.staged"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={draft !== null} onOpenChange={(open) => { if (!open) closeDraft(); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("mcp.draft.title", { count: servers.length })}</DialogTitle>
          <DialogDescription>{t(draft?.fromModel ? "mcp.draft.fromModel" : "mcp.draft.check")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-5">
          {servers.map((server, index) => (
            <section key={index} className="grid gap-3 rounded-md border border-border p-3">
              <McpServerForm idPrefix={`draft-${index}`} server={server} onChange={(next) => setServers(servers.map((item, at) => (at === index ? next : item)))} />
              {servers.length > 1 && (
                <Button type="button" variant="ghost" size="sm" className="justify-self-end" onClick={() => setServers(servers.filter((_, at) => at !== index))}>{t("mcp.remove")}</Button>
              )}
            </section>
          ))}
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={closeDraft}>{t("common.cancel")}</Button>
          <Button type="button" disabled={saving || servers.length === 0} onClick={() => void apply()}>{t("mcp.apply")}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
