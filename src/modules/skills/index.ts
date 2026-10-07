import { create } from "zustand";
import { commands, type Skill, type SkillHit } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { pickFolder } from "@/modules/organizations";
import { t } from "@/modules/i18n";

/** As skills instaladas neste computador. O Jev escolhe uma por pedido, pelo
 * nome e pela descrição; só as ligadas entram na escolha. */
interface SkillsState {
  skills: Skill[] | null;
  installing: boolean;
  /** A busca no skills.sh: `null` antes da primeira. */
  hits: SkillHit[] | null;
  searching: boolean;
  /** A skill do skills.sh que está sendo baixada (`dono/repo/nome`). */
  downloading: string | null;
}

export const useSkills = create<SkillsState>(() => ({ skills: null, installing: false, hits: null, searching: false, downloading: null }));

export async function loadSkills() {
  try {
    useSkills.setState({ skills: await commands.getSkills() });
  } catch (error) {
    reportError(error);
    useSkills.setState({ skills: [] });
  }
}

async function change(action: () => Promise<Skill[]>): Promise<boolean> {
  try {
    useSkills.setState({ skills: await action() });
    return true;
  } catch (error) {
    reportError(error);
    return false;
  }
}

/** Abre o seletor de pastas e instala a skill (ou as skills) da pasta escolhida. */
export async function installSkillFromFolder(): Promise<boolean> {
  const folder = await pickFolder(t("skills.pick")).catch(() => null);
  if (!folder) return false;
  useSkills.setState({ installing: true });
  try {
    return await change(() => commands.installSkillFolder(folder));
  } finally {
    useSkills.setState({ installing: false });
  }
}

/** Instala a skill a partir do texto de um `SKILL.md` colado. */
export function installSkillFromText(text: string): Promise<boolean> {
  return text.trim() ? change(() => commands.installSkillText(text)) : Promise.resolve(false);
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

/** Baixa e instala a skill de um resultado do skills.sh: ela vira uma skill
 * comum, com o mesmo liga e desliga e a mesma escolha do Jev. */
export async function installFromSkillHub(hit: SkillHit): Promise<boolean> {
  useSkills.setState({ downloading: hit.id });
  try {
    return await change(() => commands.installHubSkill(hit.source, hit.name));
  } finally {
    useSkills.setState({ downloading: null });
  }
}

export const setSkillEnabled = (name: string, enabled: boolean) => change(() => commands.setSkillEnabled(name, enabled));
export const removeSkill = (name: string) => change(() => commands.removeSkill(name));
export type { Skill };
