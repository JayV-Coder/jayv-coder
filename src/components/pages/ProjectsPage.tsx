import { chatsOf, createChat, deleteProject, openProject, setLayout, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { LayoutSwitch, PageHeading } from "@/components/molecules";
import { NewProjectDialog, ProjectCard } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function ProjectsPage() {
  const t = useT();
  const { data, layout } = useWorkspace();
  return (
    <ScrollPage>
      <PageHeading eyebrow={t("projects.eyebrow")} title={t("nav.projects")} description={t("projects.description")}>
        <LayoutSwitch value={layout} onChange={setLayout} />
        <NewProjectDialog><Button>{t("projects.new")}</Button></NewProjectDialog>
      </PageHeading>
      {data.projects.length === 0
        ? <EmptyText>{t("projects.empty")}</EmptyText>
        : (
          <div className={cn("grid gap-4", layout === "grid" ? "grid-cols-[repeat(auto-fill,minmax(280px,1fr))]" : "grid-cols-1")}>
            {data.projects.map((project) => (
              <ProjectCard
                key={project.id}
                project={project}
                chats={chatsOf(data, project.id)}
                onOpen={() => openProject(project.id)}
                onNewChat={() => void createChat(project.id)}
                onDelete={() => void deleteProject(project.id)}
              />
            ))}
          </div>
        )}
    </ScrollPage>
  );
}
