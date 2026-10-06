import { useState } from "react";
import { ShieldCheckIcon, XIcon } from "lucide-react";
import { commands, type Chat, type Grants } from "@/modules/core";
import { grantsOf, hasGrants, setGrants, useConversation } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

const KINDS = ["shell", "git", "network"] as const;

/** As permissões do próximo pedido, ao lado do modo: rodar comandos, mexer no
 * Git e usar a rede, só para a mensagem que sair em seguida. Sem nada ligado,
 * o agente que esbarrar numa permissão pergunta no painel de cima se executa,
 * nega ou sempre permite. Embaixo, os comandos sempre permitidos no projeto,
 * para tirar o que não deve mais passar sem pergunta. */
export function GrantsPicker({ chat }: { chat: Chat }) {
  const t = useT();
  const grants = grantsOf(chat.id, useConversation((state) => state.grants));
  const [always, setAlways] = useState<string[] | null>(null);
  const active = KINDS.filter((kind) => grants[kind]).length;

  const load = (open: boolean) => {
    if (!open) return;
    commands.allowedCommands(chat.id).then(setAlways, (error) => { reportError(error); setAlways([]); });
  };
  const forget = (command: string) => {
    commands.forgetAllowedCommand(chat.id, command).then(setAlways, reportError);
  };
  const toggle = (kind: (typeof KINDS)[number], on: boolean) => setGrants(chat.id, { ...grants, [kind]: on } as Grants);

  return (
    <Popover onOpenChange={load}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          title={t("grants.title")}
          className={cn("h-6 gap-1.5 px-2 text-caption", hasGrants(grants) && "border-accent text-accent")}
        >
          <ShieldCheckIcon aria-hidden="true" className="size-3.5" />
          {active > 0 ? t("grants.active", { count: active }) : t("grants.label")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="grid w-80 gap-2.5 font-sans">
        <div className="grid gap-1">
          <p className="text-sm font-semibold">{t("grants.title")}</p>
          <p className="text-xs leading-snug text-muted-foreground">{t("grants.hint")}</p>
        </div>
        {KINDS.map((kind) => (
          <ToggleRow
            key={kind}
            id={`grant-${kind}`}
            label={t(`grants.${kind}`)}
            hint={t(`grants.${kind}.hint`)}
            checked={grants[kind]}
            onChange={(on) => toggle(kind, on)}
            className="px-3 py-2"
          />
        ))}
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
