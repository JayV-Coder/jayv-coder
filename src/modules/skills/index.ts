import { create } from "zustand";
import { commands, type Skill } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { pickFolder } from "@/modules/organizations";
import { t } from "@/modules/i18n";

/** As skills instaladas neste computador. O Jev escolhe uma por pedido, pelo
 * nome e pela descrição; só as ligadas entram na escolha. */
interface SkillsState {
  skills: Skill[] | null;
  installing: boolean;
}

export const useSkills = create<SkillsState>(() => ({ skills: null, installing: false }));

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

export const setSkillEnabled = (name: string, enabled: boolean) => change(() => commands.setSkillEnabled(name, enabled));
export const removeSkill = (name: string) => change(() => commands.removeSkill(name));
export type { Skill };
