import { useEffect, useState } from "react";
import { SearchIcon } from "lucide-react";
import type { SearchHit } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { SEARCH_MIN_CHARS, searchChats, snippetParts } from "@/modules/memory";
import { chatTitle, openChat } from "@/modules/workspace";
import { Input } from "@/components/ui/input";

const DEBOUNCE_MS = 250;

/** A busca nas conversas do projeto: acha o chat pelo que se disse nele. */
export function ChatSearch({ projectId }: { projectId: string }) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);

  useEffect(() => {
    const text = query.trim();
    if (text.length < SEARCH_MIN_CHARS) { setHits(null); return; }
    let current = true;
    const timer = setTimeout(() => {
      searchChats(projectId, text).then((found) => { if (current) setHits(found); }, reportError);
    }, DEBOUNCE_MS);
    return () => { current = false; clearTimeout(timer); };
  }, [projectId, query]);

  return (
    <section className="mb-6 grid gap-2">
      <label className="relative block">
        <span className="sr-only">{t("search.label")}</span>
        <SearchIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input type="search" value={query} placeholder={t("search.placeholder")} className="pl-9" onChange={(event) => setQuery(event.target.value)} />
      </label>
      {hits && (hits.length === 0
        ? <p className="text-sm text-muted-foreground">{t("search.empty")}</p>
        : (
          <ul className="grid gap-1.5">
            {hits.map((hit) => (
              <li key={hit.chatId}>
                <button type="button" onClick={() => openChat(hit.chatId)} className="grid w-full gap-0.5 rounded-md border border-border px-3.5 py-2 text-left hover:bg-secondary">
                  <span className="text-sm font-medium text-foreground">{chatTitle(hit)}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {snippetParts(hit.snippet).map((part, index) => part.hit
                      ? <mark key={index} className="rounded-sm bg-warning/20 px-0.5 text-foreground">{part.text}</mark>
                      : <span key={index}>{part.text}</span>)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ))}
    </section>
  );
}
