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
    <article className="mb-7 rounded-[18px] border border-[#2b322d] bg-[linear-gradient(140deg,rgba(35,46,38,.72),rgba(18,22,19,.7))] p-8">
      <Eyebrow>{project ? project.name : t("welcome.eyebrow")}</Eyebrow>
      <h2 className="mb-2 text-[29px] font-bold tracking-[-0.03em]">{t(project ? "welcome.title.project" : "welcome.title.none")}</h2>
      <p className="max-w-[580px] leading-relaxed text-[#9ca69e]">
        {t(project ? "welcome.body.project" : "welcome.body.none")}
      </p>
      {project && !chat && <Button className="mt-4" onClick={() => void createChat(project.id)}>{t("common.newChat")}</Button>}
      {!project && <Button className="mt-4" onClick={() => navigate("projects")}>{t("welcome.viewProjects")}</Button>}
    </article>
  );
}
