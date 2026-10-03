import { commands, type NoteDraft, type ProjectMemory, type ProjectNote, type SearchHit } from "@/modules/core";

/** A memória do projeto: notas que o agente lê ao começar uma sessão,
 * receitas que vão junto dos pedidos parecidos e a busca nas conversas. Tudo
 * mora no núcleo; aqui só se pede e se mostra. */
export const loadMemory = (projectId: string): Promise<ProjectMemory> => commands.projectMemory(projectId);
export const saveNote = (draft: NoteDraft): Promise<ProjectMemory> => commands.saveProjectNote(draft);
export const deleteNote = (note: ProjectNote): Promise<ProjectMemory> => commands.deleteProjectNote(note.projectId, note.id);
export const searchChats = (projectId: string, query: string): Promise<SearchHit[]> => commands.searchChats(projectId, query);

/** Quantos caracteres as notas já ocupam do teto. */
export const notesUsed = (memory: ProjectMemory): number =>
  memory.notes.filter((note) => note.kind === "note").reduce((sum, note) => sum + [...note.body].length, 0);

/** A busca só sai a partir desta quantidade de letras. */
export const SEARCH_MIN_CHARS = 3;

const MARK_START = "\u0002";
const MARK_END = "\u0003";

/** O trecho achado em pedaços, com o que casou marcado para destaque. */
export function snippetParts(snippet: string): { text: string; hit: boolean }[] {
  const parts: { text: string; hit: boolean }[] = [];
  for (const chunk of snippet.split(MARK_START)) {
    const [hit, rest] = chunk.includes(MARK_END) ? chunk.split(MARK_END, 2) : [null, chunk];
    if (hit) parts.push({ text: hit, hit: true });
    if (rest) parts.push({ text: rest, hit: false });
  }
  return parts;
}
