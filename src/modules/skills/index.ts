import { create } from "zustand";
import { commands, type Skill, type SkillHit, type SkillPreview } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { pickFolder } from "@/modules/organizations";
import { t } from "@/modules/i18n";

/** Uma instalação que espera o Salvar das configurações: da pasta escolhida,
 * do texto colado ou do skills.sh. `skills` é o que ela traz (lido do
 * `SKILL.md` já na escolha; do skills.sh, só o nome); na pasta com várias,
 * `skip` são as que a pessoa tirou da lista antes de salvar. */
export type SkillInstall =
  | { kind: "folder"; path: string; skills: SkillPreview[]; skip: string[] }
  | { kind: "text"; text: string; skills: SkillPreview[] }
  | { kind: "hub"; id: string; source: string; skills: SkillPreview[] };

/** O que muda no Salvar: as skills gravadas que saem, as instalações (na
 * ordem em que foram pedidas) e o liga/desliga de cada uma. */
export interface SkillChanges {
  removed: string[];
  installs: SkillInstall[];
  enabled: Record<string, boolean>;
}

/** Uma linha da lista como a tela a mostra, com as mudanças pendentes. */
export interface SkillRow {
  name: string;
  description: string;
  enabled: boolean;
  /** Entra (ou é trocada) só no Salvar. */
  pending: boolean;
  /** Do skills.sh: o repositório `dono/repo`, mostrado no lugar da descrição. */
  source?: string;
}

/** As skills instaladas neste computador e as mudanças que ainda esperam o
 * Salvar das configurações. O Jev escolhe uma por pedido, pelo nome e pela
 * descrição; só as ligadas entram na escolha. */
interface SkillsState {
  skills: Skill[] | null;
  changes: SkillChanges;
  /** Lendo a pasta ou o texto antes de pôr a skill na lista. */
  installing: boolean;
  /** Aplicando as mudanças no Salvar. */
  saving: boolean;
  /** A busca no skills.sh: `null` antes da primeira. */
  hits: SkillHit[] | null;
  searching: boolean;
  /** A skill do skills.sh que está sendo baixada no Salvar (`dono/repo/nome`). */
  downloading: string | null;
}

const NO_CHANGES: SkillChanges = { removed: [], installs: [], enabled: {} };

export const useSkills = create<SkillsState>(() => ({ skills: null, changes: NO_CHANGES, installing: false, saving: false, hits: null, searching: false, downloading: null }));

const installed = (install: SkillInstall) => install.kind === "folder" ? install.skills.filter((skill) => !install.skip.includes(skill.name)) : install.skills;

/** A lista como ficará ao salvar, sem o liga/desliga pendente. */
function baseRows(skills: Skill[], changes: SkillChanges): Map<string, SkillRow> {
  const rows = new Map<string, SkillRow>();
  for (const skill of skills) {
    if (!changes.removed.includes(skill.name)) rows.set(skill.name, { name: skill.name, description: skill.description, enabled: skill.enabled, pending: false });
  }
  for (const install of changes.installs) {
    for (const skill of installed(install)) {
      // Reinstalar mantém o liga/desliga de quem continua; a que saiu antes volta ligada.
      rows.set(skill.name, { ...skill, enabled: rows.get(skill.name)?.enabled ?? true, pending: true, source: install.kind === "hub" ? install.source : undefined });
    }
  }
  return rows;
}

/** A lista que a tela mostra, em ordem de nome. */
export function skillRows(skills: Skill[], changes: SkillChanges): SkillRow[] {
  const rows = baseRows(skills, changes);
  for (const [name, enabled] of Object.entries(changes.enabled)) {
    const row = rows.get(name);
    if (row) rows.set(name, { ...row, enabled });
  }
  return [...rows.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export const isSkillsDirty = (state: Pick<SkillsState, "changes">) =>
  state.changes.removed.length > 0 || state.changes.installs.length > 0 || Object.keys(state.changes.enabled).length > 0;

function change(update: (changes: SkillChanges, skills: Skill[]) => SkillChanges) {
  useSkills.setState((state) => ({ changes: update(state.changes, state.skills ?? []) }));
}

/** Relê o que está instalado. A mudança pendente continua de pé. */
export async function loadSkills() {
  try {
    useSkills.setState({ skills: await commands.getSkills() });
  } catch (error) {
    reportError(error);
    useSkills.setState((state) => ({ skills: state.skills ?? [] }));
  }
}

/** Põe na lista a instalação (que espera o Salvar). Devolve se pôs. */
function stage(install: SkillInstall): boolean {
  if (install.skills.length === 0) return false;
  change((changes) => {
    const names = new Set(install.skills.map((skill) => skill.name));
    // O liga/desliga pendente de quem é reinstalado é refeito a partir do que fica.
    const enabled = Object.fromEntries(Object.entries(changes.enabled).filter(([name]) => !names.has(name)));
    return { ...changes, installs: [...changes.installs, install], enabled };
  });
  return true;
}

/** Abre o seletor de pastas e põe na lista a skill (ou as skills) da pasta
 * escolhida, já conferidas; a cópia só acontece no Salvar. */
export async function installSkillFromFolder(): Promise<boolean> {
  const folder = await pickFolder(t("skills.pick")).catch(() => null);
  if (!folder) return false;
  useSkills.setState({ installing: true });
  try {
    return stage({ kind: "folder", path: folder, skills: await commands.previewSkills({ path: folder }), skip: [] });
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useSkills.setState({ installing: false });
  }
}

/** Põe na lista a skill do texto de um `SKILL.md` colado, já conferido. */
export async function installSkillFromText(text: string): Promise<boolean> {
  if (!text.trim()) return false;
  try {
    return stage({ kind: "text", text, skills: await commands.previewSkills({ text }) });
  } catch (error) {
    reportError(error);
    return false;
  }
}

/** Procura no skills.sh. Vale a última busca: uma resposta atrasada de outra
 * pesquisa não troca a lista. */
let lastQuery = "";
export async function searchSkillHub(query: string) {
  const text = query.trim();
  lastQuery = text;
  if (text.length < 2) { useSkills.setState({ hits: null, searching: false }); return; }
  useSkills.setState({ searching: true });
  try {
    const hits = await commands.searchSkillHub(text);
    if (lastQuery === text) useSkills.setState({ hits });
  } catch (error) {
    reportError(error);
  } finally {
    if (lastQuery === text) useSkills.setState({ searching: false });
  }
}

/** Põe na lista a skill de um resultado do skills.sh: no Salvar ela é
 * baixada e vira uma skill comum, com o mesmo liga e desliga e a mesma
 * escolha do Jev. */
export function installFromSkillHub(hit: SkillHit): boolean {
  return stage({ kind: "hub", id: hit.id, source: hit.source, skills: [{ name: hit.name, description: "" }] });
}

/** Liga ou desliga uma skill (fica pendente até o Salvar). Voltar ao valor de
 * antes tira a pendência. */
export function setSkillEnabled(name: string, enabled: boolean) {
  change((changes, skills) => {
    const base = baseRows(skills, changes).get(name);
    const next = { ...changes.enabled };
    if (base?.enabled === enabled) delete next[name];
    else next[name] = enabled;
    return { ...changes, enabled: next };
  });
}

/** Tira a skill da lista (fica pendente até o Salvar). A que só ia ser
 * instalada sai da instalação; a gravada é removida no Salvar. */
export function removeSkill(name: string) {
  change((changes, skills) => {
    const installs = changes.installs.flatMap((install): SkillInstall[] => {
      if (!install.skills.some((skill) => skill.name === name)) return [install];
      if (install.kind !== "folder") return [];
      const skip = install.skip.includes(name) ? install.skip : [...install.skip, name];
      return install.skills.every((skill) => skip.includes(skill.name)) ? [] : [{ ...install, skip }];
    });
    const enabled = { ...changes.enabled };
    delete enabled[name];
    const removed = skills.some((skill) => skill.name === name) && !changes.removed.includes(name) ? [...changes.removed, name] : changes.removed;
    return { removed, installs, enabled };
  });
}

/** Aplica as mudanças pendentes, na ordem: remove as que saíram, instala as
 * novas (tirando da pasta as que a pessoa deixou de fora) e acerta o liga e
 * desliga. Uma falha para no meio: o que já foi feito fica, o resto continua
 * pendente, e a lista é relida. */
export async function saveSkillChanges(): Promise<boolean> {
  const { changes, skills } = useSkills.getState();
  if (!isSkillsDirty(useSkills.getState())) return true;
  const left: SkillChanges = { removed: [...changes.removed], installs: [...changes.installs], enabled: { ...changes.enabled } };
  let list = skills ?? [];
  useSkills.setState({ saving: true });
  try {
    while (left.removed.length > 0) {
      list = await commands.removeSkill(left.removed[0]);
      left.removed.shift();
    }
    while (left.installs.length > 0) {
      const install = left.installs[0];
      if (install.kind === "folder") list = await commands.installSkillFolder(install.path);
      else if (install.kind === "text") list = await commands.installSkillText(install.text);
      else {
        useSkills.setState({ downloading: install.id });
        list = await commands.installHubSkill(install.source, install.skills[0].name);
      }
      if (install.kind === "folder") for (const name of install.skip) list = await commands.removeSkill(name);
      left.installs.shift();
    }
    for (const [name, enabled] of Object.entries(left.enabled)) {
      const skill = list.find((item) => item.name === name);
      if (skill && skill.enabled !== enabled) list = await commands.setSkillEnabled(name, enabled);
      delete left.enabled[name];
    }
    useSkills.setState({ skills: list, changes: NO_CHANGES });
    return true;
  } catch (error) {
    reportError(error);
    useSkills.setState({ changes: left });
    void loadSkills();
    return false;
  } finally {
    useSkills.setState({ saving: false, downloading: null });
  }
}

/** Joga fora as mudanças pendentes e relê a lista instalada. */
export function discardSkills() {
  useSkills.setState({ changes: NO_CHANGES });
  void loadSkills();
}

export type { Skill };
