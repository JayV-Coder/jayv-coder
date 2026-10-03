import { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronDownIcon, RefreshCwIcon } from "lucide-react";
import type { Chat, Project, RepositoryState } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import {
  chatRepositories, organizationRules, repositoryLabel, repositoryStates, type ChatRepository, type PolicySource, type Repository, type StoredPolicy,
} from "@/modules/organizations";
import { openTurns } from "@/modules/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const POLICY: Record<PolicySource, Key> = {
  repository: "chatRepos.policy.repository",
  organization: "chatRepos.policy.organization",
  none: "chatRepos.policy.none",
};

const OPEN_KEY = "jayv.chatRepos.open";

function remembered(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) !== "0";
  } catch {
    return true;
  }
}

/** Os repositórios que o chat da organização alcança, no topo da conversa,
 * como um `git status` de cada um: o branch, quanto está à frente ou atrás do
 * remoto, quantos arquivos mudaram e de onde vem a política de LLM dele. Os
 * repositórios da organização que não estão na pasta aparecem apagados, no
 * fim. Relê ao abrir o chat, quando um pedido termina e pelo botão. */
export function RepositoryPanel({ project, chat }: { project: Project; chat: Chat | null }) {
  const t = useT();
  const [states, setStates] = useState<RepositoryState[] | null>(null);
  const [rules, setRules] = useState<{ repositories: Repository[]; policies: StoredPolicy[] }>({ repositories: [], policies: [] });
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(remembered);
  const orgId = project.orgId ?? null;
  const folder = project.rootPath;
  const running = openTurns(chat).length;

  // O que o git diz aparece logo; o cadastro e as políticas da organização
  // chegam do servidor quando chegarem, sem segurar a lista.
  useEffect(() => {
    if (orgId) void organizationRules(orgId).then(setRules);
  }, [orgId]);

  const load = useCallback(async () => {
    if (!orgId || !folder.trim()) return;
    setBusy(true);
    try {
      setStates(await repositoryStates(folder));
      setFailed(false);
    } catch (error) {
      console.error("repository states", error);
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }, [orgId, folder]);

  // Um pedido que acabou pode ter mexido nos arquivos: relê quando a fila
  // esvazia, e não a cada passo do agente.
  useEffect(() => {
    if (running === 0) void load();
  }, [load, running]);

  const toggle = () => {
    setOpen((value) => {
      try { localStorage.setItem(OPEN_KEY, value ? "0" : "1"); } catch { /* fica só nesta sessão */ }
      return !value;
    });
  };

  const items = useMemo(() => (states ? chatRepositories(states, rules.repositories, rules.policies) : null), [states, rules]);

  if (!orgId || !folder.trim()) return null;
  const inside = items?.filter((item) => item.state) ?? [];
  const changed = inside.filter((item) => (item.state?.changed ?? 0) > 0).length;

  return (
    <section aria-label={t("chatRepos.title")} className="mt-2 flex-none rounded-sm border border-border bg-card font-mono text-small">
      <header className="flex items-center gap-2 px-3 py-1.5">
        <button type="button" onClick={toggle} aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 text-start outline-none focus-visible:ring-1 focus-visible:ring-ring">
          <span aria-hidden="true" className="text-primary">❯</span>
          <span className="truncate text-foreground">{t("chatRepos.title")}</span>
          {items && (
            <span className="truncate text-muted-foreground">
              {t("chatRepos.count", { count: inside.length })}
              {changed > 0 && ` · ${t("chatRepos.dirty", { count: changed })}`}
            </span>
          )}
          <ChevronDownIcon aria-hidden="true" className={cn("ms-auto size-3.5 shrink-0 text-muted-foreground transition-transform", !open && "-rotate-90")} />
        </button>
        <Button size="icon-xs" variant="ghost" disabled={busy} aria-label={t("chatRepos.refresh")} title={t("chatRepos.refresh")} onClick={() => void load()}>
          <RefreshCwIcon aria-hidden="true" className={cn(busy && "animate-spin motion-reduce:animate-none")} />
        </Button>
      </header>
      {open && (
        <div className="border-t border-border px-3 py-2">
          {failed && <p className="text-muted-foreground">{t("chatRepos.failed")}</p>}
          {!failed && items === null && <p className="text-muted-foreground">{t("chatRepos.loading")}</p>}
          {!failed && items?.length === 0 && <p className="text-muted-foreground">{t("chatRepos.empty")}</p>}
          {!failed && items && items.length > 0 && (
            <ul className="grid grid-cols-[minmax(0,max-content)_minmax(0,1fr)_auto] items-baseline gap-x-4 gap-y-1 sm:grid-cols-[minmax(0,max-content)_minmax(0,1fr)_auto_auto]">
              {items.map((item) => <RepositoryLine key={item.key} item={item} />)}
            </ul>
          )}
          {!failed && items?.some((item) => item.policy !== "none") && <p className="mt-2 text-caption text-faint">{t("chatRepos.policy.hint")}</p>}
        </div>
      )}
    </section>
  );
}

function RepositoryLine({ item }: { item: ChatRepository }) {
  const t = useT();
  const { state } = item;
  const label = repositoryLabel(item);
  if (!state) {
    return (
      <li className="contents text-faint">
        <span className="truncate" title={item.key}>{label}</span>
        <span className="col-span-2 sm:col-span-3">{t("chatRepos.outside")}</span>
      </li>
    );
  }
  const sync = [state.ahead > 0 && `↑${state.ahead}`, state.behind > 0 && `↓${state.behind}`].filter(Boolean).join(" ");
  return (
    <li className="contents">
      <span className="truncate text-foreground" title={state.path}>{label || state.key || state.path}</span>
      <span className="truncate text-muted-foreground" title={state.upstream ?? undefined}>
        {state.readable ? (state.branch ?? t("chatRepos.detached")) : t("chatRepos.unreadable")}
        {sync && <span className="ms-2 text-ask">{sync}</span>}
      </span>
      <span className={cn("whitespace-nowrap", state.changed > 0 ? "text-ask" : "text-go")}>
        {!state.readable ? "" : state.changed > 0 ? `● ${t("chatRepos.changed", { count: state.changed })}` : `✓ ${t("chatRepos.clean")}`}
      </span>
      <span className="hidden whitespace-nowrap text-muted-foreground sm:inline">
        {item.repository ? t(POLICY[item.policy]) : t("chatRepos.unregistered")}
      </span>
    </li>
  );
}
