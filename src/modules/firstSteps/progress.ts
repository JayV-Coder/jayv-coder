/** Os primeiros 5 minutos: abrir a pasta do projeto, fazer o primeiro pedido e
 * ver o primeiro resultado comprovado pelo validador. Só a conta; quem ouve os
 * eventos é o `index.ts`. */

export const FIRST_STEPS = ["folder", "request", "verified"] as const;
export type FirstStep = (typeof FIRST_STEPS)[number];

export interface Progress {
  /** Quando o caminho começou (a primeira abertura do app com ele). */
  startedAt: number | null;
  /** Quando cada passo ficou pronto. */
  done: Partial<Record<FirstStep, number>>;
  /** Quem fechou o caminho, ou quem já usava o app antes dele. */
  dismissed: boolean;
  /** O tempo até o primeiro resultado já foi enviado. */
  reported: boolean;
}

export const EMPTY: Progress = { startedAt: null, done: {}, dismissed: false, reported: false };

/** Lê o que foi gravado, aceitando só o que tem a forma certa. */
export function parse(raw: string | null): Progress {
  try {
    const value = JSON.parse(raw ?? "null") as Partial<Progress> | null;
    if (!value || typeof value !== "object") return EMPTY;
    const done: Progress["done"] = {};
    for (const step of FIRST_STEPS) {
      const at = value.done?.[step];
      if (typeof at === "number" && Number.isFinite(at)) done[step] = at;
    }
    return {
      startedAt: typeof value.startedAt === "number" ? value.startedAt : null,
      done,
      dismissed: value.dismissed === true,
      reported: value.reported === true,
    };
  } catch {
    return EMPTY;
  }
}

/** Marca o passo, uma vez só. O resultado comprovado conta também os passos
 * de antes, que ele implica. */
export function mark(progress: Progress, step: FirstStep, now: number): Progress {
  if (progress.done[step] !== undefined) return progress;
  const done = { ...progress.done };
  for (const earlier of FIRST_STEPS.slice(0, FIRST_STEPS.indexOf(step) + 1)) done[earlier] ??= now;
  return { ...progress, startedAt: progress.startedAt ?? now, done };
}

export const complete = (progress: Progress) => FIRST_STEPS.every((step) => progress.done[step] !== undefined);

/** O próximo passo a fazer; nulo quando acabou. */
export const nextStep = (progress: Progress): FirstStep | null => FIRST_STEPS.find((step) => progress.done[step] === undefined) ?? null;

/** Quantos segundos do começo até o primeiro resultado comprovado. */
export function secondsToVerified(progress: Progress): number | null {
  const verified = progress.done.verified;
  if (verified === undefined || progress.startedAt === null) return null;
  return Math.max(0, Math.round((verified - progress.startedAt) / 1000));
}

/** O caminho aparece para quem está começando, até acabar ou ser fechado. */
export const showing = (progress: Progress) => !progress.dismissed && !complete(progress);
