import { commands, type Activity, type ValidationCommands, type ValidationSetup } from "@/modules/core";

/** O resultado da validação de um build, montado a partir dos eventos do turno
 * (`checkpoint`, `validate_*`, `repair`, `rolled_back`, `commit_held`; ver
 * `jayv-base/src/progress.rs`). Os mesmos eventos contam o turno aberto e o
 * fechado, então há uma só leitura deles. */

export type StepKind = "test" | "lint" | "typecheck" | "build";
export const STEP_KINDS: StepKind[] = ["test", "lint", "typecheck", "build"];
export type StepStatus = "pending" | "passed" | "failed" | "timed_out" | "blocked" | "stopped" | "could_not_start";
export type Verdict = "passed" | "failed" | "unproven";

export interface ValidationStep {
  kind: StepKind;
  command: string;
  status: StepStatus;
  exitCode: number | null;
  millis: number;
  /** O fim da saída, só do que não passou. */
  tail: string;
  /** As organizações que bloquearam o comando. */
  blockedBy: string[];
}

export interface ValidationState {
  /** O build não mudou arquivo nenhum e nada foi validado. */
  skipped: "unchanged" | null;
  /** O fim da última rodada; nulo enquanto ela corre. */
  verdict: Verdict | null;
  attempt: number;
  maxRepairs: number;
  /** Quantos reparos o JayV pediu ao agente. */
  repairs: number;
  steps: ValidationStep[];
  /** Os tipos de etapa que o projeto não define. */
  undefinedKinds: StepKind[];
  /** O ponto de volta gravado antes do build. */
  checkpoint: { id: string; commit: string } | null;
  /** A pasta já voltou a esse ponto. */
  rolledBack: { files: number; headMoved: boolean } | null;
  /** Comandos de Git que a organização reteve, e quem. */
  held: { commands: string[]; orgs: string[] } | null;
}

const text = (value: unknown) => (typeof value === "string" ? value : "");
const count = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : 0);
const list = (value: unknown) => (Array.isArray(value) ? value.map(String) : []);
const kindOf = (value: unknown): StepKind | null => STEP_KINDS.find((kind) => kind === value) ?? null;

/** O estado da validação de um turno, ou nada se o turno não construiu. */
export function validationOf(events: Activity[]): ValidationState | null {
  let found = false;
  const state: ValidationState = {
    skipped: null, verdict: null, attempt: 0, maxRepairs: 0, repairs: 0, steps: [], undefinedKinds: [],
    checkpoint: null, rolledBack: null, held: null,
  };
  for (const event of [...events].sort((a, b) => a.seq - b.seq)) {
    const d = (event.detail ?? {}) as Record<string, unknown>;
    switch (event.kind) {
      case "checkpoint":
        found = true;
        if (text(d.id) && text(d.commit)) state.checkpoint = { id: text(d.id), commit: text(d.commit) };
        break;
      case "validate_start": {
        found = true;
        state.skipped = null;
        state.verdict = null;
        state.attempt = count(d.attempt);
        state.maxRepairs = count(d.maxRepairs);
        state.undefinedKinds = list(d.undefined).flatMap((kind) => kindOf(kind) ?? []);
        const planned = Array.isArray(d.steps) ? (d.steps as Record<string, unknown>[]) : [];
        state.steps = planned.flatMap((step) => {
          const kind = kindOf(step.kind);
          return kind ? [{ kind, command: text(step.command), status: "pending" as const, exitCode: null, millis: 0, tail: "", blockedBy: [] }] : [];
        });
        break;
      }
      case "validate_step": {
        found = true;
        const kind = kindOf(d.step);
        if (!kind) break;
        const done: ValidationStep = {
          kind, command: text(d.command), status: (text(d.status) || "failed") as StepStatus,
          exitCode: typeof d.exitCode === "number" ? d.exitCode : null, millis: count(d.millis), tail: text(d.tail), blockedBy: list(d.blockedBy),
        };
        const at = state.steps.findIndex((step) => step.kind === kind);
        if (at >= 0) state.steps[at] = done; else state.steps.push(done);
        break;
      }
      case "validate_end": {
        found = true;
        const verdict = text(d.verdict);
        state.verdict = verdict === "passed" || verdict === "failed" ? verdict : "unproven";
        break;
      }
      case "validate_skipped":
        found = true;
        state.skipped = "unchanged";
        break;
      case "repair":
        found = true;
        state.repairs = count(d.attempt);
        state.maxRepairs = count(d.max) || state.maxRepairs;
        break;
      case "rolled_back":
        found = true;
        state.rolledBack = { files: count(d.files), headMoved: d.headMoved === true };
        break;
      case "commit_held":
        state.held = { commands: list(d.commands), orgs: list(d.orgs) };
        break;
      default:
    }
  }
  return found ? state : null;
}

/** Só o teste dá a prova: sem ele, nada passou. */
export function lacksTest(state: ValidationState): boolean {
  return state.undefinedKinds.includes("test") || (state.steps.length > 0 && !state.steps.some((step) => step.kind === "test"));
}

export type { Rollback, ValidationCommands, ValidationSetup } from "@/modules/core";

export const loadValidationSetup = (chatId: string): Promise<ValidationSetup> => commands.validationSetup(chatId);
export const saveValidationCommands = (chatId: string, manual: ValidationCommands): Promise<ValidationSetup> => commands.saveValidationCommands(chatId, manual);
export const rollbackCheckpoint = (turnId: string) => commands.rollbackCheckpoint(turnId);

/** Quanto tempo uma etapa levou, em palavras curtas. */
export function stepDuration(millis: number): string {
  if (millis < 1000) return `${millis} ms`;
  const seconds = millis / 1000;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
  return `${Math.floor(seconds / 60)} min ${Math.round(seconds % 60)} s`;
}
