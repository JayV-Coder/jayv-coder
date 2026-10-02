import type { Chat, Project } from "@/modules/core";
import { navigate } from "@/modules/navigation";
import { createChat } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { BrandMark, Eyebrow } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** A conversa vazia: convida a escrever ou a abrir um projeto, no meio da
 * coluna, onde os olhos estão antes da primeira mensagem. */
export function Welcome({ project, chat }: { project: Project | null; chat: Chat | null }) {
  const t = useT();
  return (
    <article className="m-auto flex max-w-[580px] flex-col items-center py-10 text-center">
      <BrandMark className="mb-5 size-11 rounded-lg text-xl" />
      <Eyebrow>{project ? project.name : t("welcome.eyebrow")}</Eyebrow>
      <h2 className="mb-2 text-h2 font-semibold tracking-tight">{t(project ? "welcome.title.project" : "welcome.title.none")}</h2>
      <p className="leading-relaxed text-muted-foreground">
        {t(project ? "welcome.body.project" : "welcome.body.none")}
      </p>
      {project && !chat && <Button className="mt-5" onClick={() => void createChat(project.id)}>{t("common.newChat")}</Button>}
      {!project && <Button className="mt-5" onClick={() => navigate("projects")}>{t("welcome.viewProjects")}</Button>}
    </article>
  );
}
