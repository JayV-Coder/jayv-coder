import { useState, type FormEvent, type ReactNode } from "react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { createProject, folderName, folderOwner, useWorkspace } from "@/modules/workspace";
import { Eyebrow } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

/** O formulário do projeto novo. A pasta é opcional; escolhida pelo seletor,
 * ela sugere o nome. */
export function NewProjectDialog({ children }: { children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [note, setNote] = useState("");
  // Quem digita o nome manda: a pasta só preenche o campo enquanto ele estiver
  // intocado ou vazio.
  const [typed, setTyped] = useState(false);
  const projects = useWorkspace((state) => state.data.projects);
  const owner = folderOwner(projects, path);

  const reset = (next: boolean) => {
    setOpen(next);
    if (next) { setName(""); setPath(""); setNote(""); setTyped(false); }
  };

  const explore = async () => {
    try {
      const { open: pick } = await import("@tauri-apps/plugin-dialog");
      const chosen = await pick({ directory: true, multiple: false, defaultPath: "/", title: t("project.pick.title") });
      if (typeof chosen === "string") {
        setPath(chosen);
        setNote("");
        if (!typed) setName(folderName(chosen));
      }
    } catch (error) {
      setNote(t("project.pick.failed"));
      console.error(error);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (owner) return;
    try {
      await createProject(name, path.trim() || null);
      setOpen(false);
    } catch (error) {
      reportError(error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={reset}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent className="sm:max-w-[480px]">
        <form onSubmit={submit} className="grid gap-5">
          <DialogHeader>
            <Eyebrow>{t("projects.new")}</Eyebrow>
            <DialogTitle className="text-xl">{t("project.new.title")}</DialogTitle>
            <DialogDescription>{t("projects.description")}</DialogDescription>
          </DialogHeader>
          <FormField label={t("project.name")} htmlFor="project-name">
            <Input
              id="project-name"
              required
              autoFocus
              value={name}
              placeholder={t("project.name.placeholder")}
              onChange={(event) => { setName(event.target.value); setTyped(Boolean(event.target.value.trim())); }}
            />
          </FormField>
          <FormField label={t("project.path")} htmlFor="project-path">
            <div className="flex gap-2">
              <Input id="project-path" value={path} placeholder={t("project.path.placeholder")} className="font-mono text-xs" onChange={(event) => setPath(event.target.value)} />
              <Button type="button" variant="outline" onClick={() => void explore()}>{t("project.explore")}</Button>
            </div>
            {note && <small className="text-xs text-destructive">{note}</small>}
            {owner && <small role="alert" className="text-xs text-destructive">{t("project.pathTaken", { name: owner.name })}</small>}
          </FormField>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("common.cancel")}</Button>
            <Button type="submit" disabled={Boolean(owner)}>{t("project.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
