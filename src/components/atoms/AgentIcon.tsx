import type { SVGProps } from "react";
import type { AgentId } from "@/modules/core";
import { ClaudeIcon, CodexIcon, CopilotIcon, CursorIcon, KiloIcon, LiteLLMIcon, OpenRouterIcon } from "./icons";

const ICONS = { claude: ClaudeIcon, codex: CodexIcon, copilot: CopilotIcon, cursor: CursorIcon, kilo: KiloIcon, openrouter: OpenRouterIcon, litellm: LiteLLMIcon };

/** A marca de cada agente, onde quer que ele apareça. */
export function AgentIcon({ agent, ...props }: { agent: AgentId } & SVGProps<SVGSVGElement>) {
  const Icon = ICONS[agent];
  return <Icon {...props} />;
}
