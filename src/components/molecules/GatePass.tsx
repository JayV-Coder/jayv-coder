import { shorten } from "@/modules/core";
import type { GatePassInfo } from "@/modules/gate";
import { useT } from "@/modules/i18n";
import { AspectDot } from "@/components/atoms";

/** O último registro da portaria num chat, numa linha. */
export function GatePass({ pass }: { pass: GatePassInfo | null }) {
  const t = useT();
  if (!pass) return <span className="text-xs text-muted-foreground italic">{t("gate.none")}</span>;
  const verdict = t(pass.verdict);
  const label = pass.rawKind === null
    ? t("gate.pass.entry", { verdict })
    : t("gate.pass.exit", { kind: pass.kind ? t(pass.kind) : pass.rawKind, verdict });
  return (
    <span data-aspect={pass.aspect} className="flex min-w-0 items-center gap-2 text-xs">
      <AspectDot aspect={pass.aspect} />
      <b className="flex-none font-semibold text-foreground">{label}</b>
      <code className="truncate font-mono text-caption text-muted-foreground">{shorten(pass.detail, 64)}</code>
    </span>
  );
}
