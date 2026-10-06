import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { ChevronDownIcon, ChevronUpIcon, XIcon } from "lucide-react";
import { clearMatches, closeChatFind, findRanges, paintMatches, setFindQuery, stepIndex, useChatFind } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const RESCAN_MS = 120;

/** A busca dentro da conversa aberta (Ctrl+F / ⌘F), no lugar da busca nativa
 * da webview: acha o que foi dito, pinta as ocorrências, e ↵ / ⇧↵ vão para a
 * próxima e a anterior. `version` muda quando a conversa muda (resposta
 * chegando), para as ocorrências serem lidas de novo. */
export function ChatFind({ root, version }: { root: RefObject<HTMLElement | null>; version: unknown }) {
  const t = useT();
  const { open, query, focus } = useChatFind();
  const field = useRef<HTMLInputElement>(null);
  const ranges = useRef<Range[]>([]);
  const [total, setTotal] = useState(0);
  const [current, setCurrent] = useState(0);
  const last = useRef({ query: "", scanned: false });

  useEffect(() => {
    if (!open) return;
    field.current?.focus();
    field.current?.select();
  }, [open, focus]);

  // Ler de novo as ocorrências quando o texto digitado ou a conversa mudam.
  useEffect(() => {
    if (!open) { clearMatches(); ranges.current = []; setTotal(0); last.current = { query: "", scanned: false }; return; }
    const timer = setTimeout(() => {
      const element = root.current;
      ranges.current = element ? findRanges(element, query) : [];
      setTotal(ranges.current.length);
      // Texto novo recomeça na primeira ocorrência; a conversa que só cresceu
      // mantém a que estava sendo vista.
      const fresh = last.current.query !== query;
      last.current = { query, scanned: true };
      setCurrent((index) => {
        const next = fresh ? 0 : Math.min(index, Math.max(ranges.current.length - 1, 0));
        paintMatches(ranges.current, next, fresh);
        return next;
      });
    }, RESCAN_MS);
    return () => clearTimeout(timer);
  }, [open, query, version, root]);

  useEffect(() => () => clearMatches(), []);

  if (!open) return null;

  const go = (direction: 1 | -1) => {
    if (ranges.current.length === 0) return;
    const next = stepIndex(current, ranges.current.length, direction);
    setCurrent(next);
    paintMatches(ranges.current, next);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); go(event.shiftKey ? -1 : 1); }
    else if (event.key === "Escape") { event.preventDefault(); closeChatFind(); }
  };

  const searched = query.trim().length > 0;
  return (
    <div role="search" data-find-skip data-tour="chat-find" className="absolute top-2 right-5 z-10 flex items-center gap-1 rounded-lg border border-border bg-card p-1.5 shadow-md animate-pending-in motion-reduce:animate-none">
      <Input
        ref={field}
        value={query}
        onChange={(event) => setFindQuery(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t("find.placeholder")}
        aria-label={t("find.label")}
        className="h-8 w-56 font-mono text-sm"
      />
      <span aria-live="polite" className="min-w-16 px-1 text-center text-xs text-muted-foreground tabular-nums">
        {searched ? (total === 0 ? t("find.none") : t("find.count", { current: current + 1, total })) : ""}
      </span>
      <Button type="button" variant="ghost" size="icon-sm" aria-label={t("find.previous")} title={t("find.previous")} disabled={total === 0} onClick={() => go(-1)}><ChevronUpIcon aria-hidden="true" /></Button>
      <Button type="button" variant="ghost" size="icon-sm" aria-label={t("find.next")} title={t("find.next")} disabled={total === 0} onClick={() => go(1)}><ChevronDownIcon aria-hidden="true" /></Button>
      <Button type="button" variant="ghost" size="icon-sm" aria-label={t("find.close")} title={t("find.close")} onClick={closeChatFind}><XIcon aria-hidden="true" /></Button>
    </div>
  );
}
