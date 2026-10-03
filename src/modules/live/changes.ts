import type { LiveChange } from "@/modules/core";

/** Junta um arquivo que mudou à lista do chat: ele sobe para o topo. */
export function withChange(files: LiveChange[], change: LiveChange): LiveChange[] {
  return [change, ...files.filter((file) => file.path !== change.path)];
}
