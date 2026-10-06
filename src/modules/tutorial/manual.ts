/** O texto em inglês de cada funcionalidade vem da própria documentação do
 * repositório (`docs/manual/features`), embutida no build: é a base do
 * tutorial quando a tradução (`docs.<id>.*`, do Supabase) ainda não chegou. */
export interface ManualFeature { id: string; title: string; summary: string; usage: string }

const files = import.meta.glob<ManualFeature>("../../../docs/manual/features/*.json", { eager: true, import: "default" });

export const MANUAL: Record<string, ManualFeature> = Object.fromEntries(Object.values(files).map((feature) => [feature.id, feature]));
