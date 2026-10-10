import type { SVGProps } from "react";
import { PuzzleIcon } from "lucide-react";
import { isCustomMod, type AgentId } from "@/modules/core";
import { ClaudeIcon, CodexIcon, CopilotIcon, CursorIcon, KiloIcon, LiteLLMIcon, OpenRouterIcon } from "./icons";

const ICONS = { claude: ClaudeIcon, codex: CodexIcon, copilot: CopilotIcon, cursor: CursorIcon, kilo: KiloIcon, openrouter: OpenRouterIcon, litellm: LiteLLMIcon };

/** A marca de cada mod, onde quer que ele apareça. O mod criado pela pessoa
 * leva a peça de quebra-cabeça dos mods. */
export function AgentIcon({ agent, ...props }: { agent: AgentId } & SVGProps<SVGSVGElement>) {
  if (isCustomMod(agent)) return <PuzzleIcon aria-hidden="true" {...props} />;
  const Icon = ICONS[agent];
  return <Icon {...props} />;
}
