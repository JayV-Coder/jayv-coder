import { useGate } from "@/modules/gate";
import { openTurns, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { GateInIcon, GateOutIcon } from "@/components/atoms";
import { EntryItem, ExitItem, GateBoard, GateLane } from "@/components/organisms";

/** A portaria: dois portões, um feed cada. */
export function GatePage() {
  const t = useT();
  const feed = useGate((state) => state.feed);
  // A ampulheta da saída lê o banco: ela acende enquanto houver pedido em
  // aberto em qualquer chat, não enquanto uma chamada estiver presa.
  const waiting = useWorkspace((state) => state.data.chats.some((chat) => openTurns(chat).length > 0));
  return (
    <div className="grid min-h-0 flex-1 grid-rows-[auto_1fr] bg-void font-plate text-foreground">
      <GateBoard />
      <div className="grid min-h-0 grid-cols-1 grid-rows-2 xl:grid-cols-2 xl:grid-rows-1">
        <GateLane
          icon={<GateInIcon className="size-full" />}
          title={t("gate.entry.title")}
          description={t("gate.entry.description")}
          count={feed.entries.length}
          empty={t("gate.entry.empty")}
        >
          {feed.entries.map((check) => <EntryItem key={check.id} check={check} />)}
        </GateLane>
        <GateLane
          icon={<GateOutIcon className="size-full" />}
          title={t("gate.exit.title")}
          description={t("gate.exit.description")}
          count={feed.exits.length}
          waiting={waiting}
          empty={t("gate.exit.empty")}
        >
          {feed.exits.map((check) => <ExitItem key={check.id} check={check} />)}
        </GateLane>
      </div>
    </div>
  );
}
