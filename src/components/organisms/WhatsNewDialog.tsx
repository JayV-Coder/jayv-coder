import { SparklesIcon, WrenchIcon } from "lucide-react";
import { closeChanges, showAllChanges, useChangelog, type ChangeKind, type Release } from "@/modules/changelog";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { installUpdate } from "@/modules/updates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

const GROUPS: { kind: ChangeKind; label: Key; Icon: typeof SparklesIcon; tone: string }[] = [
  { kind: "feature", label: "whatsNew.features", Icon: SparklesIcon, tone: "text-success" },
  { kind: "fix", label: "whatsNew.fixes", Icon: WrenchIcon, tone: "text-info" },
];

/** Uma versão: o número, o dia e os itens em dois grupos, novidades primeiro.
 * Um grupo vazio não aparece. */
function ReleaseNotes({ release, installed }: { release: Release; installed: boolean }) {
  const t = useT();
  // Item de uma versão que este app ainda não conhece: sem a chave, vale o
  // inglês que veio nas notas do release.
  const say = (key: string, fallback?: string | null) => {
    const text = t(key as Key);
    return text === key && fallback ? fallback : text;
  };
  const locale = useLocale();
  // A data é só o dia: lida como meio-dia UTC, não muda de dia em fuso nenhum.
  const day = new Intl.DateTimeFormat(locale, { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" }).format(new Date(`${release.date}T12:00:00Z`));
  return (
    <section className="grid gap-3" aria-labelledby={`release-${release.version}`}>
      <header className="flex flex-wrap items-center gap-2">
        <h3 id={`release-${release.version}`} className="text-h4 font-semibold">{t("whatsNew.version", { version: release.version })}</h3>
        {installed && <Badge variant="success">{t("whatsNew.installed")}</Badge>}
        <span className="text-caption text-muted-foreground">{day}</span>
      </header>
      {GROUPS.map(({ kind, label, Icon, tone }) => {
        const items = release.items.filter((item) => item.kind === kind);
        if (!items.length) return null;
        return (
          <div key={kind} className="grid gap-2">
            <strong className="flex items-center gap-1.5 text-xs tracking-wider text-muted-foreground uppercase">
              <Icon aria-hidden="true" className={`size-3.5 ${tone}`} />
              {t(label)}
            </strong>
            <ul className="grid gap-2.5">
              {items.map((item) => (
                <li key={item.id} className="rounded-md border border-border bg-muted/40 px-3.5 py-2.5">
                  <p className="text-sm font-medium text-foreground">{say(`whatsNew.item.${item.id}.title`, item.title)}</p>
                  <p className="mt-1 text-sm text-muted-foreground">{say(`whatsNew.item.${item.id}.detail`, item.detail)}</p>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </section>
  );
}

/** O que mudou: abre sozinha depois de uma atualização, com as versões que a
 * pessoa ainda não viu, pelo botão "Novidades" com o histórico inteiro, e pelo
 * "O que muda" do aviso de atualização só com a versão nova, com o botão de
 * atualizar. */
export function WhatsNewDialog() {
  const t = useT();
  const { open, current, previous, releases, all, upcoming } = useChangelog();
  if (!current) return null;
  const description = upcoming
    ? t("whatsNew.description.upcoming", { version: upcoming })
    : previous
    ? t("whatsNew.description.updated", { previous, version: current })
    : all ? t("whatsNew.description") : t("whatsNew.description.installed", { version: current });
  return (
    <Dialog open={open} onOpenChange={(wanted) => { if (!wanted) closeChanges(); }}>
      <DialogContent className="sm:max-w-[600px]">
        <DialogHeader>
          <DialogTitle className="text-xl">{t("whatsNew.title", { version: upcoming ?? current })}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="-mx-1 grid max-h-[60vh] gap-6 overflow-y-auto px-1">
          {releases.map((release) => <ReleaseNotes key={release.version} release={release} installed={release.version === current} />)}
        </div>

        <DialogFooter>
          {!all && <Button variant="ghost" onClick={showAllChanges}>{t("whatsNew.showAll")}</Button>}
          {upcoming ? (
            <>
              <Button variant="ghost" onClick={closeChanges}>{t("whatsNew.done")}</Button>
              <Button onClick={() => { closeChanges(); void installUpdate(); }}>{t("update.install")}</Button>
            </>
          ) : <Button onClick={closeChanges}>{t("whatsNew.done")}</Button>}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
