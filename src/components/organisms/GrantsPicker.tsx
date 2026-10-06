import { useEffect, useState } from "react";
import { ShieldCheckIcon, XIcon } from "lucide-react";
import { commands, type Chat, type Grants } from "@/modules/core";
import { useIntentHandler } from "@/modules/commands";
import { grantsOf, hasGrants, loadGrants, setGrants, useConversation } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { blockersOf, COMMAND_CATALOG, conflictsWith, useOrganizations } from "@/modules/organizations";
import { ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

const KINDS = ["shell", "git", "network"] as const;
const NONE: Record<string, string[]> = {};

/** As permissões do chat, ao lado do modo: rodar comandos, mexer no Git e
 * usar a rede, ligadas para todas as mensagens do chat até serem desligadas. Sem nada ligado,
 * o agente que esbarrar numa permissão pergunta no painel de cima se executa,
 * nega ou sempre permite. Embaixo, os comandos sempre permitidos no projeto,
 * para tirar o que não deve mais passar sem pergunta.
 *
 * Num projeto de organização, o que ela bloqueia (`Permissões` no site) fica
 * desativado com "Bloqueado por <organização>": nem "rodar comandos" nem "o
 * Git inteiro" valem com comando bloqueado, e cada comando se libera um a um. */
export function GrantsPicker({ chat }: { chat: Chat }) {
  const t = useT();
  const grants = grantsOf(chat.id, useConversation((state) => state.grants));
  const [always, setAlways] = useState<string[] | null>(null);
  const [open, setOpen] = useState(false);
  const blocked = useOrganizations((state) => state.blockedCommands[chat.projectId]) ?? NONE;
  const anyBlocked = Object.keys(blocked).length > 0;
  const orgsOf = (rules: string[]) => [...new Set(rules.flatMap((rule) => blockersOf(blocked, rule)))];
  const gitBlockers = orgsOf(Object.keys(blocked).filter((rule) => rule.split(" ")[0] === "git"));
  const allBlockers = orgsOf(Object.keys(blocked));
  const locked = (kind: (typeof KINDS)[number]) => (kind === "shell" ? allBlockers : kind === "git" ? gitBlockers : []);
  const active = KINDS.filter((kind) => grants[kind]).length + grants.commands.length;
  useEffect(() => { void loadGrants(chat.id); }, [chat.id]);
  useIntentHandler("commandPermissions", () => { setOpen(true); load(true); });

  const load = (opening: boolean) => {
    if (!opening) return;
    commands.allowedCommands(chat.id).then(setAlways, (error) => { reportError(error); setAlways([]); });
  };
  const forget = (command: string) => {
    commands.forgetAllowedCommand(chat.id, command).then(setAlways, reportError);
  };
  const toggle = (kind: (typeof KINDS)[number], on: boolean) => setGrants(chat.id, { ...grants, [kind]: on } as Grants);
  const toggleCommand = (rule: string, on: boolean) => {
    const rest = grants.commands.filter((command) => command !== rule);
    setGrants(chat.id, { ...grants, commands: on ? [...rest, rule] : rest });
  };

  return (
    <Popover open={open} onOpenChange={(next) => { setOpen(next); load(next); }}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          title={t("grants.title")}
          data-tour="composer-grants"
          className={cn("h-6 gap-1.5 px-2 text-caption", hasGrants(grants) && "border-accent text-accent")}
        >
          <ShieldCheckIcon aria-hidden="true" className="size-3.5" />
          {active > 0 ? t("grants.active", { count: active }) : t("grants.label")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="grid max-h-[70vh] w-80 gap-2.5 overflow-y-auto font-sans">
        <div className="grid gap-1">
          <p className="text-sm font-semibold">{t("grants.title")}</p>
          <p className="text-xs leading-snug text-muted-foreground">{t("grants.hint")}</p>
        </div>
        {KINDS.map((kind) => {
          const orgs = locked(kind);
          return (
            <ToggleRow
              key={kind}
              id={`grant-${kind}`}
              label={t(`grants.${kind}`)}
              hint={orgs.length > 0 ? t("grants.blockedBy", { orgs: orgs.join(", ") }) : t(`grants.${kind}.hint`)}
              checked={grants[kind] && orgs.length === 0}
              disabled={orgs.length > 0}
              onChange={(on) => toggle(kind, on)}
              className="px-3 py-2"
            />
          );
        })}
        <div className="grid gap-1.5 border-t border-border pt-2.5">
          <p className="text-xs font-semibold">{t("grants.commands")}</p>
          <p className="text-xs leading-snug text-muted-foreground">{anyBlocked ? t("grants.commands.blockedHint") : t("grants.commands.hint")}</p>
          <div className="grid gap-1">
            {COMMAND_CATALOG.map(({ tool, subcommands }) => (
              <CommandGroup key={tool} tool={tool} subcommands={subcommands} selected={grants.commands} blocked={blocked} onToggle={toggleCommand} />
            ))}
          </div>
        </div>
        <div className="grid gap-1.5 border-t border-border pt-2.5">
          <p className="text-xs font-semibold">{t("grants.always")}</p>
          {always === null ? null : always.length === 0 ? (
            <p className="text-xs text-muted-foreground">{t("grants.always.none")}</p>
          ) : (
            <ul className="grid gap-1">
              {always.map((command) => (
                <li key={command} className="flex items-center justify-between gap-2 rounded-md border border-border/60 px-2 py-1">
                  <code className="min-w-0 truncate font-mono text-xs">{command}</code>
                  <Button type="button" variant="ghost" size="icon-sm" aria-label={t("grants.forget", { command })} title={t("grants.forget", { command })} onClick={() => forget(command)}>
                    <XIcon aria-hidden="true" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <p className="text-xs leading-snug text-muted-foreground">{t("grants.always.hint")}</p>
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** Um programa do catálogo: ele inteiro e cada subcomando, para liberar um a um. */
function CommandGroup({ tool, subcommands, selected, blocked, onToggle }: {
  tool: string; subcommands: string[]; selected: string[]; blocked: Record<string, string[]>; onToggle: (rule: string, on: boolean) => void;
}) {
  const t = useT();
  const rows = [tool, ...subcommands.map((sub) => `${tool} ${sub}`)];
  const count = rows.filter((rule) => selected.includes(rule)).length;
  const row = (rule: string) => {
    const orgs = rule === tool ? conflictsWith(blocked, rule) : blockersOf(blocked, rule);
    const label = rule === tool ? (subcommands.length > 0 ? t("grants.command.all", { tool }) : tool) : rule.slice(tool.length + 1);
    return (
      <label key={rule} htmlFor={`grant-cmd-${rule}`} className={cn("flex items-center gap-2 rounded-md px-1.5 py-1 text-xs", orgs.length > 0 && "opacity-55")}>
        <Checkbox id={`grant-cmd-${rule}`} checked={orgs.length === 0 && selected.includes(rule)} disabled={orgs.length > 0} onCheckedChange={(on) => onToggle(rule, on === true)} />
        <code className="min-w-0 truncate font-mono">{label}</code>
        {orgs.length > 0 && <span className="ms-auto flex-none text-caption text-muted-foreground">{t("grants.blockedBy", { orgs: orgs.join(", ") })}</span>}
      </label>
    );
  };
  if (subcommands.length === 0) return row(tool);
  return (
    <details className="rounded-md border border-border/60 px-2 py-1">
      <summary className="cursor-pointer select-none font-mono text-xs">{tool}{count > 0 && <span className="ms-1.5 text-accent">({count})</span>}</summary>
      <div className="mt-1 grid gap-0.5">{rows.map(row)}</div>
    </details>
  );
}
