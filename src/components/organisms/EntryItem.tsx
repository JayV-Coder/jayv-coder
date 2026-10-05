import { useState } from "react";
import type { EntryCheck } from "@/modules/core";
import { ENTRY_VERDICTS, sourceLabel } from "@/modules/conversation";
import { useGate } from "@/modules/gate";
import { useT, type Key } from "@/modules/i18n";
import { openChat, useWorkspace } from "@/modules/workspace";
import { ChevronIcon, SignalHead3Icon, Stamp } from "@/components/atoms";
import { ChatRef, Gauge, scopeKey } from "@/components/molecules";
import { cn } from "@/lib/utils";
import { GateItem } from "./GateItem";

const NOTES: Record<EntryCheck["verdict"], Key> = { pass: "entry.note.pass", ask: "entry.note.ask", block: "entry.note.block" };

/** Um pedido que passou pelo portão de entrada: nota, mínimo e, ao abrir, os
 * critérios que deram a nota. */
export function EntryItem({ check }: { check: EntryCheck }) {
  const t = useT();
  const [opened, setOpened] = useState(false);
  // Fora do projeto aberto (a portaria da organização), o chat vem da área
  // de trabalho inteira.
  const projectChat = useGate((state) => state.chats[check.chatId]);
  const anyChat = useWorkspace((state) => projectChat ? undefined : state.data.chats.find((found) => found.id === check.chatId));
  const chat = projectChat ?? anyChat;
  const { aspect, label } = ENTRY_VERDICTS[check.verdict] ?? ENTRY_VERDICTS.block;
  const panelId = `criteria-${check.id}`;
  // O núcleo escreve a nota e o tamanho em português; os dois são refeitos
  // aqui a partir do veredito, no idioma de quem lê.
  const known = scopeKey(check.scope);
  const scope = known ? t(known) : check.scope;
  // O pedido que o desenvolvedor confirmou depois de a portaria perguntar
  // passa com a nota própria.
  const confirmed = check.note === "entry.note.confirmed";
  const note = known ? t(confirmed ? "entry.note.confirmed" : NOTES[check.verdict] ?? "entry.note.block", { scope }) : check.note;
  return (
    <GateItem id={check.id} aspect={aspect}>
      <button
        type="button"
        aria-expanded={opened}
        aria-controls={panelId}
        onClick={() => setOpened(!opened)}
        className="flex w-full items-start gap-[17px] px-5 pt-[15px] pb-4 text-start hover:bg-rail focus-visible:outline-2 focus-visible:-outline-offset-[3px] focus-visible:outline-ring"
      >
        <SignalHead3Icon className="w-6 flex-none" />
        <span className="grid min-w-0 flex-1 gap-2">
          <span className="flex flex-wrap items-center gap-x-[13px] gap-y-1.5">
            <Stamp at={check.at} />
            <span className="text-sm font-semibold text-[var(--aspect)]">{t(label)}</span>
            <span className="ms-auto flex items-baseline gap-[9px] font-gate-mono">
              <b className="text-2xl font-semibold text-foreground tabular-nums">{check.score}</b>
              <i className="text-xs text-faint not-italic">{t("entry.min", { demand: check.demand })}</i>
            </span>
          </span>
          <p className={cn("max-w-[64ch] overflow-hidden font-gate-mono text-sm leading-[1.6] text-foreground", !opened && "line-clamp-3")}>{check.prompt}</p>
          <p className="max-w-[58ch] text-sm leading-normal text-dim">{note}</p>
        </span>
        <ChevronIcon className={cn("mt-[3px] w-[15px] flex-none text-faint transition-transform duration-200 motion-reduce:transition-none", opened && "rotate-180")} />
      </button>
      <div className="-mt-1.5 flex pe-5 pb-[13px] ps-[61px]">
        <ChatRef chat={chat} onOpen={() => openChat(check.chatId)} />
      </div>
      {opened && (
        <div id={panelId} className="border-t border-rail bg-void pe-6 pb-[18px] ps-6 xl:ps-[61px]">
          <div className="flex flex-wrap justify-between gap-x-[18px] gap-y-1.5 pt-[13px] pb-1.5 text-sm text-dim">
            <span>{t("entry.demand", { scope, demand: check.demand })}</span>
            <span className="font-gate-mono text-faint">{t("entry.source", { source: sourceLabel(check.source) })}</span>
          </div>
          {check.criteria.map((criterion) => <Gauge key={criterion.id} criterion={criterion} />)}
        </div>
      )}
    </GateItem>
  );
}
