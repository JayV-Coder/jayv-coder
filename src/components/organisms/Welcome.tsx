import type { Chat, Project } from "@/modules/core";
import { navigate } from "@/modules/navigation";
import { createChat } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { Eyebrow } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** A conversa vazia: convida a escrever ou a abrir um projeto. */
export function Welcome({ project, chat }: { project: Project | null; chat: Chat | null }) {
  const t = useT();
  return (
    <article className="mb-7 rounded-lg border border-border bg-card p-6">
      <Eyebrow>{project ? project.name : t("welcome.eyebrow")}</Eyebrow>
      <h2 className="mb-2 text-h2 font-semibold tracking-tight">{t(project ? "welcome.title.project" : "welcome.title.none")}</h2>
      <p className="max-w-[580px] leading-relaxed text-muted-foreground">
        {t(project ? "welcome.body.project" : "welcome.body.none")}
      </p>
      {project && !chat && <Button className="mt-4" onClick={() => void createChat(project.id)}>{t("common.newChat")}</Button>}
      {!project && <Button className="mt-4" onClick={() => navigate("projects")}>{t("welcome.viewProjects")}</Button>}
    </article>
  );
}
