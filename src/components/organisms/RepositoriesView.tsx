import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckIcon, CopyIcon, RefreshCwIcon } from "lucide-react";
import type { Chat, Project, RepositoryState } from "@/modules/core";
import { isText, say, useT, type Key } from "@/modules/i18n";
import {
  chatRepositories, organizationRules, repositoryLabel, repositoryStates, safeDirectoryCommand, type ChatRepository, type PolicySource, type Repository,
  type StoredPolicy,
} from "@/modules/organizations";
import { openTurns } from "@/modules/workspace";
import { Button } from "@/components/ui/button";
import { LoadingNote } from "@/components/atoms";
import { cn } from "@/lib/utils";

const POLICY: Record<PolicySource, Key> = {
  repository: "chatRepos.policy.repository",
  organization: "chatRepos.policy.organization",
  none: "chatRepos.policy.none",
};

/** Os repositórios da pasta do chat, na aba Repositórios do painel de código,
 * como um `git status` de cada um: o branch, quanto está à frente ou atrás do
 * remoto e quantos arquivos mudaram. A pasta que está dentro de um
 * repositório (a subpasta de um monorepo) mostra o repositório em volta. No
 * chat de uma organização, cada um diz também de onde vem a política de LLM
 * dele, e os repositórios da organização que não estão na pasta aparecem
 * apagados, no fim. Quando o git não responde, a linha diz por quê (sem git,
 * pasta de outro dono, demora) e como resolver. Lê ao abrir (com ou sem
 * pedido andando), relê quando um pedido termina e pelo botão. */
export function RepositoriesView({ project, chat }: { project: Project; chat: Chat }) {
  const t = useT();
  const [states, setStates] = useState<RepositoryState[] | null>(null);
  const [rules, setRules] = useState<{ repositories: Repository[]; policies: StoredPolicy[] }>({ repositories: [], policies: [] });
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const orgId = project.orgId ?? null;
  const folder = project.rootPath;
  const running = openTurns(chat).length;

  // O que o git diz aparece logo; o cadastro e as políticas da organização
  // chegam do servidor quando chegarem, sem segurar a lista.
  useEffect(() => {
    if (orgId) void organizationRules(orgId).then(setRules);
  }, [orgId]);

  const load = useCallback(async () => {
    if (!folder.trim()) return;
    setBusy(true);
    try {
      setStates(await repositoryStates(folder));
      setFailed(null);
    } catch (error) {
      console.error("repository states", error);
      // O núcleo diz o motivo (a pasta sumiu, por exemplo) já escrito para a tela.
      setFailed(isText(error) ? say(error) : error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [folder]);

  // Lê ao abrir, mesmo com um pedido andando (antes a aba esperava o pedido
  // acabar e ficava em "Lendo…" o tempo todo), e de novo quando a fila
  // esvazia: o pedido que acabou pode ter mexido nos arquivos. Não relê a
  // cada passo do agente.
  useEffect(() => { void load(); }, [load]);
  const before = useRef(running);
  useEffect(() => {
    if (before.current > 0 && running === 0) void load();
    before.current = running;
  }, [load, running]);

  const items = useMemo(() => (states ? chatRepositories(states, orgId ? rules.repositories : [], orgId ? rules.policies : []) : null), [states, rules, orgId]);
  const inside = items?.filter((item) => item.state) ?? [];
  const changed = inside.filter((item) => (item.state?.changed ?? 0) > 0).length;
  const noGit = inside.length > 0 && inside.every((item) => item.state?.problem === "noGit");

  return (
    <section aria-label={t("chatRepos.title")} className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-none items-center gap-2 border-b border-border px-3 py-1">
        <span className="min-w-0 flex-1 truncate text-caption text-faint" title={folder}>
          {items ? t("chatRepos.count", { count: inside.length }) : t("chatRepos.loading")}
          {changed > 0 && ` · ${t("chatRepos.dirty", { count: changed })}`}
        </span>
        <Button size="icon-xs" variant="ghost" disabled={busy} aria-label={t("chatRepos.refresh")} title={t("chatRepos.refresh")} onClick={() => void load()}>
          <RefreshCwIcon aria-hidden="true" className={cn(busy && "animate-spin motion-reduce:animate-none")} />
        </Button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {failed && <p role="alert" className="text-muted-foreground [overflow-wrap:anywhere]">{t("chatRepos.failed")} <span className="text-faint">{failed}</span></p>}
        {!failed && items === null && <LoadingNote fill={false}>{t("chatRepos.loading")}</LoadingNote>}
        {!failed && items?.length === 0 && <p className="text-muted-foreground">{t("chatRepos.empty")}</p>}
        {!failed && noGit && <p role="alert" className="mb-2 text-ask">{t("chatRepos.noGit")}</p>}
        {!failed && items && items.length > 0 && (
          <ul className="grid gap-3">
            {items.map((item) => <RepositoryLine key={item.key} item={item} organization={orgId !== null} quietGit={noGit} />)}
          </ul>
        )}
        {!failed && items?.some((item) => item.policy !== "none") && <p className="mt-2 text-caption text-faint">{t("chatRepos.policy.hint")}</p>}
      </div>
    </section>
  );
}

/** Um comando que a pessoa copia para rodar no terminal. */
function CopyCommand({ command }: { command: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard?.writeText(command).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }).catch(() => undefined);
  };
  return (
    <span className="mt-0.5 flex items-start gap-1 rounded-xs border border-border bg-muted/50 px-1.5 py-1">
      <code className="min-w-0 flex-1 text-caption break-all text-foreground select-all">{command}</code>
      <Button size="icon-xs" variant="ghost" aria-label={t("chatRepos.copy")} title={t("chatRepos.copy")} onClick={copy}>
        {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
      </Button>
    </span>
  );
}

function RepositoryLine({ item, organization, quietGit }: { item: ChatRepository; organization: boolean; quietGit: boolean }) {
  const t = useT();
  const { state } = item;
  const label = repositoryLabel(item);
  if (!state) {
    return (
      <li className="grid gap-0.5 text-faint">
        <span className="truncate" title={item.key}>{label}</span>
        <span>{t("chatRepos.outside")}</span>
      </li>
    );
  }
  const sync = [state.ahead > 0 && `↑${state.ahead}`, state.behind > 0 && `↓${state.behind}`].filter(Boolean).join(" ");
  return (
    <li className="grid gap-0.5">
      <span className="truncate text-foreground" title={state.path}>{label || state.path}</span>
      {state.enclosing && <span className="text-caption text-faint">{t("chatRepos.enclosing")}</span>}
      {state.readable ? (
        <>
          <span className="truncate text-muted-foreground" title={state.upstream ?? undefined}>
            {state.branch ?? t("chatRepos.detached")}
            {sync && <span className="ms-2 text-ask">{sync}</span>}
          </span>
          <span className="flex flex-wrap gap-x-3 text-caption">
            <span className={cn("whitespace-nowrap", state.changed > 0 ? "text-ask" : "text-go")}>
              {state.changed > 0 ? `● ${t("chatRepos.changed", { count: state.changed })}` : `✓ ${t("chatRepos.clean")}`}
            </span>
            {organization && <span className="text-muted-foreground">{item.repository ? t(POLICY[item.policy]) : t("chatRepos.unregistered")}</span>}
          </span>
        </>
      ) : (
        <span className="grid gap-0.5 text-caption">
          {state.problem === "unsafe" ? (
            <>
              <span className="text-ask">{t("chatRepos.unsafe")}</span>
              <CopyCommand command={safeDirectoryCommand(state.path)} />
            </>
          ) : state.problem === "noGit" ? (!quietGit && <span className="text-ask">{t("chatRepos.noGit")}</span>)
            : state.problem === "timeout" ? <span className="text-ask">{t("chatRepos.timeout")}</span>
            : <span className="text-ask [overflow-wrap:anywhere]">{state.error ? t("chatRepos.gitError", { reason: state.error }) : t("chatRepos.unreadable")}</span>}
        </span>
      )}
    </li>
  );
}
