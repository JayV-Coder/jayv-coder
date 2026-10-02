import type { ExitCheck } from "@/modules/core";
import { EXIT_VERDICTS } from "@/modules/conversation";
import { EXIT_KINDS, useGate } from "@/modules/gate";
import { useT } from "@/modules/i18n";
import { openChat } from "@/modules/workspace";
import { CommandIcon, FileIcon, HouseRuleIcon, SignalHead2Icon, Stamp } from "@/components/atoms";
import { ChatRef } from "@/components/molecules";
import { cn } from "@/lib/utils";
import { GateItem } from "./GateItem";

const KIND_ICONS: Record<string, typeof FileIcon> = { command: CommandIcon, file: FileIcon, "comando": CommandIcon, "arquivo": FileIcon };

/** O que o modelo pediu para rodar ou mexer, e a regra da casa que isso tocou. */
export function ExitItem({ check }: { check: ExitCheck }) {
  const t = useT();
  const chat = useGate((state) => state.chats[check.chatId]);
  const { aspect, label } = EXIT_VERDICTS[check.verdict] ?? EXIT_VERDICTS.held;
  const Kind = KIND_ICONS[check.kind] ?? FileIcon;
  return (
    <GateItem id={check.id} aspect={aspect}>
      <div className="flex w-full items-start gap-[17px] px-5 pt-[15px] pb-4">
        <SignalHead2Icon className="w-6 flex-none" />
        <span className="grid min-w-0 flex-1 gap-2">
          <span className="flex flex-wrap items-center gap-x-[13px] gap-y-1.5">
            <Stamp at={check.at} />
            <span className="flex items-center gap-1.5 text-sm text-dim"><Kind className="size-[15px] flex-none" />{EXIT_KINDS[check.kind] ? t(EXIT_KINDS[check.kind]) : check.kind}</span>
            <span className="text-sm font-semibold text-[var(--aspect)]">{t(label)}</span>
          </span>
          <code className="block max-w-[64ch] border-s-2 border-rail-2 bg-rail px-[11px] py-2 font-gate-mono text-sm leading-[1.55] [overflow-wrap:anywhere] text-foreground">{check.target}</code>
          <p className={cn("flex items-center gap-2 font-gate-mono text-sm text-foreground", !check.rule && "font-plate text-sm text-faint")}>
            <HouseRuleIcon className={cn("size-[15px] flex-none", !check.rule && "opacity-45")} />
            <span>{check.rule ?? t("exit.noRule")}</span>
          </p>
        </span>
      </div>
      <div className="-mt-1.5 flex pe-5 pb-[13px] ps-[61px]">
        <ChatRef chat={chat} onOpen={() => openChat(check.chatId)} />
      </div>
    </GateItem>
  );
}
