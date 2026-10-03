import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { PencilIcon, RepeatIcon, Trash2Icon } from "lucide-react";
import type { NoteDraft, NoteKind, ProjectMemory, ProjectNote } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { deleteNote, loadMemory, notesUsed, saveNote } from "@/modules/memory";
import { Eyebrow } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";

const TITLE_WORDS = 6;
const blank = (projectId: string, kind: NoteKind): NoteDraft => ({ id: null, projectId, kind, title: "", body: "", trigger: "" });

/** O formulário de uma nota ou receita, nova ou em edição. */
function NoteForm({ draft, onSave, onCancel }: { draft: NoteDraft; onSave: (draft: NoteDraft) => Promise<void>; onCancel?: () => void }) {
  const t = useT();
  const [value, setValue] = useState(draft);
  useEffect(() => setValue(draft), [draft]);
  const recipe = value.kind === "recipe";
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    await onSave(value);
    if (!value.id) setValue(blank(value.projectId, value.kind));
  };
  return (
    <form onSubmit={submit} className="grid gap-3 rounded-md border border-border p-3">
      {recipe && (
        <>
          <FormField label={t("memory.recipe.name")} htmlFor="recipe-title">
            <Input id="recipe-title" value={value.title} placeholder={t("memory.recipe.name.placeholder")} onChange={(event) => setValue({ ...value, title: event.target.value })} />
          </FormField>
          <FormField label={t("memory.recipe.trigger")} htmlFor="recipe-trigger">
            <Input id="recipe-trigger" value={value.trigger} placeholder={t("memory.recipe.trigger.placeholder")} onChange={(event) => setValue({ ...value, trigger: event.target.value })} />
          </FormField>
        </>
      )}
      <FormField label={recipe ? t("memory.recipe.steps") : t("memory.note.text")} htmlFor={`${value.kind}-body`}>
        <Textarea
          id={`${value.kind}-body`}
          rows={recipe ? 5 : 2}
          value={value.body}
          placeholder={recipe ? t("memory.recipe.steps.placeholder") : t("memory.note.placeholder")}
          onChange={(event) => setValue({ ...value, body: event.target.value })}
        />
      </FormField>
      <div className="flex justify-end gap-2">
        {onCancel && <Button type="button" variant="ghost" size="sm" onClick={onCancel}>{t("common.cancel")}</Button>}
        <Button type="submit" size="sm" disabled={!value.body.trim() || (recipe && !value.title.trim())}>
          {value.id ? t("memory.save") : recipe ? t("memory.recipe.add") : t("memory.note.add")}
        </Button>
      </div>
    </form>
  );
}

/** Uma nota ou receita gravada, com editar e apagar. */
function NoteItem({ note, onSave, onDelete }: { note: ProjectNote; onSave: (draft: NoteDraft) => Promise<void>; onDelete: () => void }) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  if (editing) {
    return (
      <NoteForm
        draft={{ id: note.id, projectId: note.projectId, kind: note.kind, title: note.title, body: note.body, trigger: note.trigger }}
        onSave={async (draft) => { await onSave(draft); setEditing(false); }}
        onCancel={() => setEditing(false)}
      />
    );
  }
  return (
    <li className="flex items-start gap-3 rounded-md border border-border bg-muted/40 px-3.5 py-2.5">
      <div className="grid min-w-0 flex-1 gap-1">
        {note.kind === "recipe" && <p className="text-sm font-medium text-foreground">{note.title}</p>}
        {note.kind === "recipe" && note.trigger && note.trigger !== note.title && (
          <p className="truncate text-xs text-muted-foreground">{t("memory.recipe.when", { trigger: note.trigger })}</p>
        )}
        <p className="text-sm whitespace-pre-wrap text-foreground">{note.body}</p>
        {note.source === "gate" && <Badge variant="secondary" className="w-fit">{t("memory.note.learned")}</Badge>}
      </div>
      <Button type="button" variant="ghost" size="icon" aria-label={t("memory.edit")} onClick={() => setEditing(true)}><PencilIcon /></Button>
      <Button type="button" variant="ghost" size="icon" aria-label={t("common.delete")} onClick={onDelete}><Trash2Icon /></Button>
    </li>
  );
}

/** A memória do projeto: as notas que o agente lê ao começar uma sessão e as
 * receitas que vão junto dos pedidos parecidos, mais os pedidos que se
 * repetem e ainda podem virar receita. */
export function ProjectMemoryDialog({ projectId, children }: { projectId: string; children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [memory, setMemory] = useState<ProjectMemory | null>(null);
  const [recipeDraft, setRecipeDraft] = useState<NoteDraft>(blank(projectId, "recipe"));

  useEffect(() => {
    if (!open) return;
    setRecipeDraft(blank(projectId, "recipe"));
    loadMemory(projectId).then(setMemory, reportError);
  }, [open, projectId]);

  const save = async (draft: NoteDraft) => {
    try { setMemory(await saveNote(draft)); } catch (error) { reportError(error); throw error; }
  };
  const remove = (note: ProjectNote) => { deleteNote(note).then(setMemory, reportError); };
  const notes = memory?.notes.filter((note) => note.kind === "note") ?? [];
  const recipes = memory?.notes.filter((note) => note.kind === "recipe") ?? [];

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent className="grid max-h-[85vh] grid-rows-[auto_1fr] sm:max-w-[640px]">
        <DialogHeader>
          <Eyebrow>{t("memory.eyebrow")}</Eyebrow>
          <DialogTitle className="text-xl">{t("memory.title")}</DialogTitle>
          <DialogDescription>{t("memory.description")}</DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="notes" className="min-h-0">
          <TabsList>
            <TabsTrigger value="notes">{t("memory.tab.notes")}</TabsTrigger>
            <TabsTrigger value="recipes">{t("memory.tab.recipes")}</TabsTrigger>
          </TabsList>
          <TabsContent value="notes" className="grid min-h-0 gap-3 overflow-y-auto pr-1">
            <p className="text-xs text-muted-foreground">{t("memory.notes.hint")}</p>
            {memory && (
              <p className="font-mono text-xs text-muted-foreground">{t("memory.notes.used", { used: notesUsed(memory), limit: memory.notesLimit })}</p>
            )}
            <ul className="grid gap-2">
              {notes.map((note) => <NoteItem key={note.id} note={note} onSave={save} onDelete={() => remove(note)} />)}
            </ul>
            {memory && notes.length === 0 && <p className="text-sm text-muted-foreground">{t("memory.notes.empty")}</p>}
            <NoteForm draft={blank(projectId, "note")} onSave={save} />
          </TabsContent>
          <TabsContent value="recipes" className="grid min-h-0 gap-3 overflow-y-auto pr-1">
            <p className="text-xs text-muted-foreground">{t("memory.recipes.hint", { limit: memory?.recipeLimit ?? 0 })}</p>
            {memory && memory.repeated.length > 0 && (
              <section className="grid gap-2">
                <strong className="flex items-center gap-1.5 text-xs tracking-wider text-muted-foreground uppercase">
                  <RepeatIcon aria-hidden="true" className="size-3.5" />
                  {t("memory.repeated")}
                </strong>
                <ul className="grid gap-2">
                  {memory.repeated.map((repeat) => (
                    <li key={repeat.prompt} className="flex items-center gap-3 rounded-md border border-dashed border-border px-3.5 py-2">
                      <span className="min-w-0 flex-1 truncate text-sm" title={repeat.prompt}>{repeat.prompt}</span>
                      <Badge variant="secondary">{t("memory.repeated.count", { count: repeat.count })}</Badge>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => setRecipeDraft({
                          ...blank(projectId, "recipe"),
                          title: repeat.prompt.split(/\s+/).slice(0, TITLE_WORDS).join(" "),
                          trigger: repeat.prompt,
                          body: repeat.prompt,
                        })}
                      >
                        {t("memory.repeated.save")}
                      </Button>
                    </li>
                  ))}
                </ul>
              </section>
            )}
            <ul className="grid gap-2">
              {recipes.map((note) => <NoteItem key={note.id} note={note} onSave={save} onDelete={() => remove(note)} />)}
            </ul>
            {memory && recipes.length === 0 && <p className="text-sm text-muted-foreground">{t("memory.recipes.empty")}</p>}
            <NoteForm draft={recipeDraft} onSave={save} />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
