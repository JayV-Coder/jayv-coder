import { CopyIcon, FolderOpenIcon } from "lucide-react";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { PathText } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** Um caminho do disco com copiar e mostrar na pasta. */
export function PathLine({ label, path }: { label: string; path: string }) {
  const t = useT();
  const copy = async () => {
    await navigator.clipboard.writeText(path);
    notify(t("system.copied"));
  };
  const reveal = async () => {
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  };
  return (
    <div className="flex items-center gap-3 border-b border-border/60 py-2 last:border-b-0">
      <span className="grid min-w-0 flex-1">
        <span className="text-xs text-muted-foreground">{label}</span>
        <PathText title={path} className="text-sm">{path}</PathText>
      </span>
      <Button variant="ghost" size="icon-sm" aria-label={t("system.path.copy")} title={t("system.path.copy")} onClick={() => void copy().catch(reportError)}>
        <CopyIcon />
      </Button>
      <Button variant="ghost" size="icon-sm" aria-label={t("system.path.reveal")} title={t("system.path.reveal")} onClick={() => void reveal().catch(reportError)}>
        <FolderOpenIcon />
      </Button>
    </div>
  );
}
