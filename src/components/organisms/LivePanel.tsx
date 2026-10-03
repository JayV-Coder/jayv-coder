import { useEffect, useMemo, useState } from "react";
import { CrosshairIcon, ExternalLinkIcon, XIcon } from "lucide-react";
import type { Chat, LiveChange, LiveFile } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import {
  diffCounts, hunks, lineDiff, liveFile, loadEditors, openLiveInEditor, selectLiveFile, setLiveEditor, setLiveFollow, setLivePanel, useLive,
  type DiffLine,
} from "@/modules/live";
import { openTurns } from "@/modules/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const GLYPH: Record<LiveChange["kind"], string> = { created: "+", modified: "~", removed: "−" };
const KIND: Record<LiveChange["kind"], Key> = { created: "live.kind.created", modified: "live.kind.modified", removed: "live.kind.removed" };
const EDITOR_NAMES: Record<string, string> = { code: "VS Code", cursor: "Cursor", windsurf: "Windsurf", "code-insiders": "VS Code Insiders", codium: "VSCodium" };
export const editorName = (editor: string) => EDITOR_NAMES[editor] ?? editor;

/** O que o agente está mexendo, ao lado da conversa: a lista de arquivos que o
 * pedido mudou (o mais recente no topo) e o diff do escolhido, contra como ele
 * estava quando o pedido começou. O diff se refaz a cada gravação. Seguindo o
 * agente, o arquivo que acabou de mudar passa a ser o mostrado; com um editor
 * escolhido, ele também abre lá. */
export function LivePanel({ chat }: { chat: Chat }) {
  const t = useT();
  const files = useLive((state) => state.chats[chat.id]?.files ?? []);
  const selected = useLive((state) => state.selected[chat.id] ?? null);
  const follow = useLive((state) => state.follow);
  const editor = useLive((state) => state.editor);
  const editors = useLive((state) => state.editors);
  const running = openTurns(chat).length > 0;
  const current = files.find((file) => file.path === selected) ?? files[0] ?? null;

  useEffect(() => { void loadEditors(); }, []);

  return (
    <aside aria-label={t("live.title")} className="flex min-h-0 w-[min(46%,640px)] min-w-[300px] flex-none flex-col border-s border-border bg-card font-mono text-small">
      <header className="flex flex-none items-center gap-2 border-b border-border px-3 py-1.5">
        <span aria-hidden="true" className="text-primary">❯</span>
        <span className="text-foreground">{t("live.title")}</span>
        <span className={cn("inline-flex items-center gap-1.5 truncate text-muted-foreground")}>
          <span aria-hidden="true" className={cn("size-1.5 rounded-full", running ? "bg-go animate-pulse motion-reduce:animate-none" : "bg-faint")} />
          {t(running ? "live.running" : "live.idle")}
        </span>
        <span className="ms-auto" />
        <Button size="icon-xs" variant={follow ? "secondary" : "ghost"} aria-pressed={follow} title={t("live.follow")} aria-label={t("live.follow")} onClick={() => setLiveFollow(!follow)}>
          <CrosshairIcon aria-hidden="true" />
        </Button>
        <Button size="icon-xs" variant="ghost" title={t("live.close")} aria-label={t("live.close")} onClick={() => setLivePanel(chat.id, false)}>
          <XIcon aria-hidden="true" />
        </Button>
      </header>
      {editors && editors.length > 0 && (
        <div className="flex flex-none flex-wrap items-center gap-2 border-b border-border px-3 py-1.5 text-caption text-muted-foreground">
          <span>{t("live.editor")}</span>
          {[null, ...editors].map((option) => (
            <button
              key={option ?? "none"}
              type="button"
              aria-pressed={editor === option}
              onClick={() => setLiveEditor(option)}
              className={cn("rounded-xs border px-1.5 outline-none focus-visible:ring-1 focus-visible:ring-ring", editor === option ? "border-primary text-foreground" : "border-border hover:text-foreground")}
            >
              {option ? editorName(option) : t("live.editor.none")}
            </button>
          ))}
        </div>
      )}
      {files.length === 0 ? (
        <p className="px-3 py-3 text-muted-foreground">{t(running ? "live.waiting" : "live.empty")}</p>
      ) : (
        <>
          <ul className="max-h-[38%] flex-none overflow-y-auto border-b border-border py-1">
            {files.map((file) => (
              <li key={file.path}>
                <button
                  type="button"
                  onClick={() => selectLiveFile(chat.id, file.path)}
                  aria-current={file.path === current?.path || undefined}
                  title={t(KIND[file.kind])}
                  className={cn("flex w-full items-baseline gap-2 px-3 py-0.5 text-start outline-none hover:bg-muted focus-visible:bg-muted", file.path === current?.path && "bg-muted text-foreground")}
                >
                  <span aria-hidden="true" className={cn("w-3 flex-none", file.kind === "created" ? "text-go" : file.kind === "removed" ? "text-stop" : "text-ask")}>{GLYPH[file.kind]}</span>
                  <span className="min-w-0 flex-1 truncate" title={file.path}>{file.path}</span>
                </button>
              </li>
            ))}
          </ul>
          {current && <FileDiff chatId={chat.id} change={current} editor={editor ?? editors?.[0] ?? null} />}
        </>
      )}
    </aside>
  );
}

/** O diff de um arquivo, refeito a cada gravação dele. */
function FileDiff({ chatId, change, editor }: { chatId: string; change: LiveChange; editor: string | null }) {
  const t = useT();
  const [view, setView] = useState<LiveFile | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    liveFile(chatId, change.path)
      .then((file) => { if (alive) { setView(file); setFailed(false); } })
      .catch((error) => { console.error("live file", error); if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [chatId, change.path, change.at]);

  const lines = useMemo(() => {
    if (!view || view.hidden || view.binary || view.tooLarge) return null;
    return lineDiff(view.before ?? "", view.after ?? "");
  }, [view]);
  const counts = lines ? diffCounts(lines) : null;
  const note = failed ? "live.failed" : !view ? null : view.hidden ? "live.hidden" : view.binary ? "live.binary" : view.tooLarge ? "live.tooLarge" : null;

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-none items-center gap-2 px-3 py-1.5">
        <span className="min-w-0 flex-1 truncate text-foreground" title={change.path}>{change.path}</span>
        {counts && (
          <span className="flex-none tabular-nums">
            <span className="text-go">+{counts.added}</span> <span className="text-stop">−{counts.removed}</span>
          </span>
        )}
        {editor && change.kind !== "removed" && (
          <Button size="icon-xs" variant="ghost" title={t("live.openIn", { editor: editorName(editor) })} aria-label={t("live.openIn", { editor: editorName(editor) })}
            onClick={() => void openLiveInEditor(chatId, change.path, editor).catch(reportError)}>
            <ExternalLinkIcon aria-hidden="true" />
          </Button>
        )}
      </header>
      {view && !view.beforeKnown && <p className="flex-none px-3 pb-1 text-caption text-faint">{t("live.noBefore")}</p>}
      <div className="min-h-0 flex-1 overflow-auto pb-3">
        {note && <p className="px-3 text-muted-foreground">{t(note)}</p>}
        {lines && <DiffBody lines={lines} />}
      </div>
    </section>
  );
}

function DiffBody({ lines }: { lines: DiffLine[] }) {
  const t = useT();
  const parts = hunks(lines);
  if (parts.length === 0) return <p className="px-3 text-muted-foreground">{t("live.same")}</p>;
  return (
    <table className="w-full border-collapse text-caption leading-5">
      <tbody>
        {parts.map((part, index) => (
          <HunkRows key={index} skipped={part.skipped} lines={part.lines} />
        ))}
      </tbody>
    </table>
  );
}

function HunkRows({ skipped, lines }: { skipped: number; lines: DiffLine[] }) {
  const t = useT();
  return (
    <>
      {skipped > 0 && (
        <tr><td colSpan={4} className="bg-muted/60 px-3 text-faint">{t("live.skipped", { count: skipped })}</td></tr>
      )}
      {lines.map((line, index) => (
        <tr
          key={index}
          className={cn(
            line.kind === "added" && "bg-[color-mix(in_srgb,var(--go)_12%,transparent)]",
            line.kind === "removed" && "bg-[color-mix(in_srgb,var(--stop)_12%,transparent)]",
          )}
        >
          <td className="w-px select-none px-1.5 text-end text-faint tabular-nums">{line.before ?? ""}</td>
          <td className="w-px select-none px-1.5 text-end text-faint tabular-nums">{line.after ?? ""}</td>
          <td aria-hidden="true" className={cn("w-px select-none pe-1", line.kind === "added" ? "text-go" : line.kind === "removed" ? "text-stop" : "text-faint")}>
            {line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}
          </td>
          <td className="whitespace-pre pe-3 text-foreground">{line.text}</td>
        </tr>
      ))}
    </>
  );
}
