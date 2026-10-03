import type { CoreSnapshot, Expertise } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { saveExpertise } from "@/modules/settings";
import { SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { cn } from "@/lib/utils";

const percent = (value: number) => `${Math.round(value * 100)}%`;

/** O nível da conta. Cada opção mostra o que ela faz com a portaria e com o
 * Jev, com os números de agora: quem escolhe vê a troca antes de fazê-la. */
export function ExpertisePanel({ snapshot }: { snapshot: CoreSnapshot }) {
  const t = useT();
  const suggestion = snapshot.suggestion;
  return (
    <SettingsSection title={t("expertise.title")} description={t("expertise.description")}>
      {suggestion && suggestion.level !== snapshot.expertise && (
        <div className="mb-3 flex flex-wrap items-center gap-3 rounded-md border border-info/30 bg-info/10 px-3.5 py-2.5">
          <span className="min-w-0 flex-1 text-sm text-foreground">
            {t("expertise.suggestion", {
              days: suggestion.days,
              passed: suggestion.history.passed,
              checks: suggestion.history.checks,
              level: t(`expertise.${suggestion.level}` as Key),
            })}
          </span>
          <Button size="sm" variant="outline" onClick={() => void saveExpertise(suggestion.level)}>
            {t("expertise.suggestion.apply", { level: t(`expertise.${suggestion.level}` as Key) })}
          </Button>
        </div>
      )}
      <RadioGroup value={snapshot.expertise} onValueChange={(value) => void saveExpertise(value as Expertise)} className="gap-2">
        {snapshot.levels.map((level) => {
          const chosen = level.id === snapshot.expertise;
          return (
            <Label
              key={level.id}
              htmlFor={`expertise-${level.id}`}
              className={cn(
                "grid cursor-pointer grid-cols-[auto_1fr] items-start gap-x-3 gap-y-1 rounded-md border border-border px-3.5 py-3 font-normal hover:bg-secondary",
                chosen && "border-muted-foreground/50 bg-secondary",
              )}
            >
              <RadioGroupItem id={`expertise-${level.id}`} value={level.id} className="mt-0.5" />
              <span className="grid gap-1">
                <strong className="text-sm">{t(`expertise.${level.id}` as Key)}</strong>
                <span className="text-xs text-muted-foreground">{t(`expertise.${level.id}.hint` as Key)}</span>
                <span className="font-mono text-xs text-muted-foreground">
                  {t("expertise.numbers", {
                    demand: level.scopeDemand.map(percent).join(" / "),
                    confidence: percent(level.confidence),
                    build: t(`complexity.${level.buildCeiling}` as Key),
                    destructive: percent(level.destructiveThreshold),
                  })}
                </span>
              </span>
            </Label>
          );
        })}
      </RadioGroup>
    </SettingsSection>
  );
}
