import type { LiveChange, LiveNotice } from "@/modules/core";

/** Junta um arquivo que mudou à lista do chat: ele sobe para o topo. O que
 * nasceu e sumiu durante o pedido sai dela. */
export function withChange(files: LiveChange[], change: LiveNotice): LiveChange[] {
  const rest = files.filter((file) => file.path !== change.path);
  return change.kind === "discarded" ? rest : [{ ...change, kind: change.kind }, ...rest];
}
