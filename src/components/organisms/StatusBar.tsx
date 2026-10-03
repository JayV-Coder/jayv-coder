import { showChanges, useChangelog } from "@/modules/changelog";
import { MOD, setPaletteOpen } from "@/modules/commands";
import { useConnection } from "@/modules/connection";
import { TALLY, useGate } from "@/modules/gate";
import { useT } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { findProject, useWorkspace } from "@/modules/workspace";
import { Kbd } from "@/components/atoms";
import { cn } from "@/lib/utils";

const GLYPH = { go: "✓", ask: "?", stop: "✕" } as const;

/** A barra de status no pé da janela, como a de um terminal ou editor: se a
 * sincronização está em dia, a pasta do projeto aberto, a contagem da portaria
 * dele, o atalho da paleta e a versão instalada. */
export function StatusBar() {
  const t = useT();
  const { link, pending, refusals } = useConnection();
  const { data, activeProjectId } = useWorkspace();
  const project = findProject(data, activeProjectId);
  const tally = useGate((state) => state.feed.tally);
  const gateProject = useGate((state) => state.project?.id ?? null);
  const version = useChangelog((state) => state.current);

  // O que a sincronização deve fica em âmbar; em dia, uma luz verde só.
  const problem = link === "offline"
    ? t("connection.offline")
    : link === "expired"
      ? t("connection.expired")
      : refusals.unplaced > 0
        ? t("connection.failed", { count: refusals.unplaced })
        : pending > 0 && link !== "online"
          ? t("connection.pending", { count: pending })
          : null;

  return (
    <footer className="flex h-6 flex-none items-center gap-4 overflow-hidden border-t border-border bg-sidebar px-3 font-mono text-caption whitespace-nowrap text-sidebar-muted">
      <span role="status" className={cn("inline-flex items-center gap-1.5", problem && "text-warning")}>
        <span aria-hidden="true" className={cn("size-1.5 rounded-full bg-go", problem && "bg-warning")} />
        {problem ?? t("status.synced")}
      </span>
      {project && (
        <>
          <span className="min-w-0 truncate" title={project.rootPath ?? undefined}>{project.rootPath ?? project.name}</span>
          {gateProject === project.id && (
            <button
              type="button"
              onClick={() => navigate("gate")}
              title={t("status.gate")}
              className="inline-flex items-center gap-2 rounded-xs px-1 outline-none hover:text-sidebar-foreground focus-visible:ring-1 focus-visible:ring-ring"
            >
              {t("nav.gate")}
              {TALLY.slice(0, 3).map(([field, aspect, label]) => (
                <span key={field} data-aspect={aspect} title={t(label)} className="text-[var(--aspect)] tabular-nums">
                  {tally[field]} {GLYPH[aspect]}
                </span>
              ))}
            </button>
          )}
        </>
      )}
      <div className="ms-auto flex items-center gap-4">
        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="inline-flex items-center gap-1.5 rounded-xs px-1 outline-none hover:text-sidebar-foreground focus-visible:ring-1 focus-visible:ring-ring"
        >
          <Kbd>{MOD}</Kbd><Kbd>K</Kbd>
          {t("palette.title")}
        </button>
        {version && (
          <button
            type="button"
            onClick={() => void showChanges()}
            title={t("palette.whatsNew")}
            className="rounded-xs px-1 tabular-nums outline-none hover:text-sidebar-foreground focus-visible:ring-1 focus-visible:ring-ring"
          >
            v{version}
          </button>
        )}
      </div>
    </footer>
  );
}
