import { ClipboardCopyIcon, RefreshCwIcon, SparklesIcon, ArrowUpCircleIcon } from "lucide-react";
import { RELEASES, showChanges } from "@/modules/changelog";
import { useConnection } from "@/modules/connection";
import { notify, reportError } from "@/modules/feedback";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { AGENT_LABELS, AGENTS, checkAllAgents, useSettings, type ProbeState } from "@/modules/settings";
import { agentHealth, connectionHealth, diagnosticReport, loadStatus, useSystem, type Health } from "@/modules/system";
import { checkForUpdate, isUpdateBusy, useUpdate } from "@/modules/updates";
import { Eyebrow, LoadingNote } from "@/components/atoms";
import { HealthLine, Metric, PageHeading, PathLine } from "@/components/molecules";
import { DatabaseCard } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

const LINK: Record<string, Key> = { online: "system.link.online", offline: "system.link.offline", expired: "system.link.expired", signedOut: "system.link.signedOut" };

/** O sistema de relance: o que está saudável (servidor e agentes), a versão
 * instalada com a busca de atualização, os números do motor, onde ficam a
 * configuração e o banco, e as tabelas do banco. "Copiar diagnóstico" junta
 * tudo num texto para colar num pedido de ajuda. */
export function StatusPage() {
  const t = useT();
  const locale = useLocale();
  const status = useSystem((state) => state.status);
  const { link, pending, refusals } = useConnection();
  const probes = useSettings((state) => state.probes);
  const agents = useSettings((state) => state.agents);
  const update = useUpdate();
  if (!status) return <LoadingNote>{t("status.loading")}</LoadingNote>;

  const number = new Intl.NumberFormat(locale);
  // Antes de a configuração carregar, nenhum agente conta como desligado.
  const configured = Object.fromEntries(agents.map((agent) => [agent.id, agent.command.trim() !== ""])) as Partial<Record<string, boolean>>;
  // Como na barra de status: as recusas de um chat ou projeto aparecem nele.
  const refused = refusals.unplaced;
  const server = connectionHealth(link, pending, refused);
  const probeOf = (probe: ProbeState) => (probe === "checking" ? null : probe);

  const agentLine = (id: (typeof AGENTS)[number]) => {
    const probe = probes[id];
    const health: Health = probe === "checking" ? "off" : configured[id] === false ? "off" : agentHealth(probeOf(probe));
    const detail = probe === "checking"
      ? t("agent.checking")
      : configured[id] === false || probe === null
        ? t("system.agent.off")
        : !probe.path ? t("system.agent.missing") : probe.version ?? t("system.agent.silent");
    return <HealthLine key={id} health={health} label={AGENT_LABELS[id]} detail={detail} title={probe && probe !== "checking" ? probe.path ?? undefined : undefined} />;
  };

  const metrics: [string, string, string?][] = [
    [t("status.providers"), number.format(status.providers)],
    [t("status.models"), number.format(status.models)],
    [t("status.indexed"), number.format(status.indexed_files)],
    [t("status.cache"), t("status.cache.value", { count: status.cache_entries })],
    [t("status.history"), t("status.history.value", { count: status.session_messages })],
    [t("system.router"), number.format(status.performance_records), t("system.router.hint")],
  ];

  const reload = () => {
    void loadStatus();
    checkAllAgents();
  };

  const copy = async () => {
    const text = diagnosticReport({
      status, link, pending, refused, platform: navigator.userAgent, language: locale, at: new Date(),
      agents: AGENTS.map((id) => ({ label: AGENT_LABELS[id], probe: configured[id] === false ? null : probeOf(probes[id]) })),
    });
    await navigator.clipboard.writeText(text);
    notify(t("system.copied"));
  };

  const released = RELEASES.find((release) => release.version === status.version);
  const updating = isUpdateBusy(update.phase);
  const updateNote = update.phase === "available" && update.next
    ? t("system.update.available", { version: update.next })
    : update.phase === "latest" ? t("update.latest") : updating ? t("system.update.running") : null;

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("system.eyebrow")} title={t("nav.system")} description={t("system.description")}>
        <Button variant="outline" onClick={reload}><RefreshCwIcon />{t("system.reload")}</Button>
        <Button onClick={() => void copy().catch(reportError)}><ClipboardCopyIcon />{t("system.copy")}</Button>
      </PageHeading>
      <div className="grid gap-4">
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)]">
          <Card className="gap-2 px-5 py-5">
            <div className="flex items-center justify-between gap-2">
              <Eyebrow className="mb-0">{t("system.health")}</Eyebrow>
              <Button variant="ghost" size="sm" onClick={() => navigate("settings")}>{t("system.agents.configure")}</Button>
            </div>
            <ul className="grid">
              <HealthLine health={server} label={t("system.server")}
                detail={[t(LINK[link] ?? "system.link.signedOut"), pending > 0 && t("connection.pending", { count: pending }), refused > 0 && t("connection.failed", { count: refused })].filter(Boolean).join(" · ")} />
              {AGENTS.map(agentLine)}
            </ul>
          </Card>
          <Card className="gap-3 px-5 py-5">
            <Eyebrow className="mb-0">{t("status.version")}</Eyebrow>
            <strong className="font-mono text-h2 font-semibold">{status.version}</strong>
            {released && (
              <p className="text-xs text-muted-foreground">
                {t("system.released", { date: new Intl.DateTimeFormat(locale, { dateStyle: "long", timeZone: "UTC" }).format(new Date(`${released.date}T00:00:00Z`)), count: released.items.length })}
              </p>
            )}
            {updateNote && <p role="status" className="text-xs text-muted-foreground">{updateNote}</p>}
            <div className="mt-auto flex flex-wrap gap-2">
              <Button variant={update.phase === "available" ? "default" : "outline"} size="sm" disabled={updating} onClick={() => void checkForUpdate(true)}>
                <ArrowUpCircleIcon />{t(update.phase === "available" ? "update.install" : "system.update.check")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => void showChanges()}><SparklesIcon />{t("system.whatsNew")}</Button>
            </div>
          </Card>
        </div>
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
          {metrics.map(([label, value, hint]) => <Metric key={label} label={label} title={hint}>{value}</Metric>)}
        </div>
        <Card className="gap-1 px-5 py-4">
          <Eyebrow className="mb-1">{t("system.files")}</Eyebrow>
          <PathLine label={t("system.files.config")} path={status.config_path} />
          <PathLine label={t("system.files.database")} path={status.database_path} />
        </Card>
        <DatabaseCard status={status} />
      </div>
    </ScrollPage>
  );
}
