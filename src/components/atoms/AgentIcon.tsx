import type { SVGProps } from "react";
import { PuzzleIcon } from "lucide-react";
import { isCustomMod, type AgentId } from "@/modules/core";
import { modOfAgent, useMods } from "@/modules/mods";
import { ClaudeIcon, CodexIcon, CopilotIcon, CursorIcon, KiloIcon, LiteLLMIcon, OpenRouterIcon } from "./icons";
import { ModIcon } from "./ModIcon";

const ICONS = { claude: ClaudeIcon, codex: CodexIcon, copilot: CopilotIcon, cursor: CursorIcon, kilo: KiloIcon, openrouter: OpenRouterIcon, litellm: LiteLLMIcon };

/** A marca de cada agente, onde quer que ele apareça. O agente que um mod do
 * catálogo trouxe (Ollama, LM Studio…) leva o ícone do mod; o que a pessoa
 * criou, a peça de quebra-cabeça dos mods. */
export function AgentIcon({ agent, ...props }: { agent: AgentId } & SVGProps<SVGSVGElement>) {
  const custom = isCustomMod(agent);
  const icon = useMods((state) => (custom ? modOfAgent(state.catalog, agent)?.icon ?? null : null));
  if (custom) return icon ? <ModIcon icon={icon} className={typeof props.className === "string" ? props.className : undefined} /> : <PuzzleIcon aria-hidden="true" {...props} />;
  const Icon = ICONS[agent];
  return <Icon {...props} />;
}
