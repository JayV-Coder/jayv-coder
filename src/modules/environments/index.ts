import { create } from "zustand";
import { commands, onCore } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { refreshWorkspace } from "@/modules/workspace";
import type { Role } from "@/modules/organizations/rules";
import { markSwitching, resumedFromSwitch } from "./switch";

/** O ambiente do pessoal; os das organizações são o id delas. */
export const PERSONAL = "personal";

/** Um ambiente da lista: o pessoal ou uma organização de quem usa o app. */
export interface EnvironmentOption {
  id: string;
  kind: "personal" | "organization";
  name: string | null;
  role: Role | null;
}

interface EnvironmentState {
  /** O ambiente do banco que o núcleo tem aberto. */
  active: string;
  switching: boolean;
}

/** Depois de uma troca, a janela abre já com o carregamento na frente, até os
 * dados do ambiente novo chegarem (`finishSwitch`). */
export const useEnvironment = create<EnvironmentState>(() => ({ active: PERSONAL, switching: resumedFromSwitch() }));

/** Os dados do ambiente novo chegaram: o carregamento sai da frente. */
export function finishSwitch() {
  if (useEnvironment.getState().switching) useEnvironment.setState({ switching: false });
}

/** Os ambientes que dá para abrir: o pessoal primeiro, depois uma organização
 * por nome. */
export function environmentOptions(organizations: { id: string; name: string; role: Role }[]): EnvironmentOption[] {
  const named = [...organizations].sort((a, b) => a.name.localeCompare(b.name));
  return [
    { id: PERSONAL, kind: "personal", name: null, role: null },
    ...named.map((org) => ({ id: org.id, kind: "organization" as const, name: org.name, role: org.role })),
  ];
}

/** O nome do ambiente: o da organização ou o texto do pessoal. */
export function environmentLabel(id: string, organizations: { id: string; name: string }[], personal: string): string {
  if (id === PERSONAL) return personal;
  return organizations.find((org) => org.id === id)?.name ?? personal;
}

/** Pergunta ao núcleo qual ambiente ele tem aberto. */
export async function loadEnvironment() {
  try {
    useEnvironment.setState({ active: await commands.currentEnvironment() });
  } catch (error) {
    console.error("environment", error);
  }
}

/** Abre o ambiente escolhido. Cada ambiente tem o seu banco, as suas
 * configurações e os seus dados, e quase todo o app guarda um pedaço deles na
 * tela: depois da troca a janela recarrega, e tudo é lido de novo do banco novo. */
export async function switchEnvironment(id: string) {
  const { active, switching } = useEnvironment.getState();
  if (id === active || switching) return;
  useEnvironment.setState({ switching: true });
  try {
    markSwitching();
    useEnvironment.setState({ active: await commands.setEnvironment(id) });
    window.location.reload();
  } catch (error) {
    useEnvironment.setState({ switching: false });
    reportError(error);
  }
}

/** Projetos de organização que saíram do banco pessoal mudam o que a lista
 * mostra: o núcleo avisa e a tela os relê. */
export function connectEnvironments() {
  const off = onCore("environment-changed", ({ environment }) => {
    useEnvironment.setState({ active: environment });
    void refreshWorkspace();
  });
  return () => void off.then((unlisten) => unlisten());
}
