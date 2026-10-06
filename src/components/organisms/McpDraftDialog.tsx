import { useEffect, useState } from "react";
import type { McpServer } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { closeDraft, confirmDraft, useMcp } from "@/modules/mcp";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { McpServerForm } from "./McpServerForm";

/** A conferência do servidor MCP antes de gravar: o que veio do `/mcp` do
 * chat, da colagem na tela ou do botão de editar. O comando roda neste
 * computador com o que o agente pedir, então nada entra sem um clique. */
export function McpDraftDialog() {
  const t = useT();
  const draft = useMcp((state) => state.draft);
  const [servers, setServers] = useState<McpServer[]>([]);
  const [saving, setSaving] = useState(false);
  useEffect(() => setServers(draft?.servers ?? []), [draft]);
  const save = async () => {
    setSaving(true);
    try { await confirmDraft(servers); } finally { setSaving(false); }
  };

  return (
    <Dialog open={draft !== null} onOpenChange={(open) => { if (!open) closeDraft(); }}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("mcp.draft.title", { count: servers.length })}</DialogTitle>
          <DialogDescription>{t(draft?.fromModel ? "mcp.draft.fromModel" : "mcp.draft.description")}</DialogDescription>
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
          <Button type="button" disabled={saving || servers.length === 0} onClick={() => void save()}>{t("mcp.save")}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
