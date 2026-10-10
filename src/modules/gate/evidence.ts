import { commands, type TurnEvidence } from "@/modules/core";
import type { Key, Params } from "@/modules/i18n";
import { agentLabel } from "@/modules/settings";
import { EXIT_KINDS } from "./store";

/** O que sustenta uma resposta, em três grupos que não se misturam: o que o
 * JayV observou com os próprios olhos, o que um modelo disse e ninguém
 * conferiu, e o que ficou sem verificação. "Não verificado" é um resultado
 * válido — melhor que um verde que ninguém mediu. */
export interface EvidenceGroups {
  observed: string[];
  inferred: string[];
  unverified: string[];
}

type Say = (key: Key, params?: Params) => string;

const COMMANDS = new Set(["command", "comando"]);
const FILES = new Set(["file", "arquivo"]);

const agentName = (provider: string) => agentLabel(provider);

export function evidenceGroups(evidence: TurnEvidence, say: Say): EvidenceGroups {
  const observed: string[] = [];
  const inferred: string[] = [];
  const unverified: string[] = [];
  const { entry, exits, review } = evidence;

  if (entry) observed.push(say("evidence.entry", { score: entry.score, demand: entry.demand, scope: say(`scope.${entry.scopeLevel}` as Key) }));
  const commands = exits.filter((exit) => COMMANDS.has(exit.kind)).length;
  const files = exits.filter((exit) => FILES.has(exit.kind)).length;
  if (commands === 0 && files === 0) observed.push(say("evidence.exit.none"));
  if (commands > 0) observed.push(say("evidence.exit.commands", { count: commands }));
  if (files > 0) observed.push(say("evidence.exit.files", { count: files }));
  for (const exit of exits.filter((check) => check.verdict === "held")) {
    const kind = EXIT_KINDS[exit.kind];
    observed.push(say("evidence.exit.held", { kind: kind ? say(kind) : exit.kind, target: exit.target, rule: exit.rule ?? "" }));
  }

  inferred.push(say("evidence.answer"));
  if (review) {
    inferred.push(review.arrived
      ? say("evidence.review", { agent: agentName(review.provider), model: review.model, count: review.files })
      : say("evidence.review.missing", { agent: agentName(review.provider) }));
  }

  unverified.push(say("evidence.noChecks"));
  if (entry) unverified.push(say(entry.doneCriterion ? "evidence.done.declared" : "evidence.done.missing"));

  return { observed, inferred, unverified };
}

export function loadEvidence(turnId: string): Promise<TurnEvidence> {
  return commands.turnEvidence(turnId);
}
