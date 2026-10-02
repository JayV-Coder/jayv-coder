/** O segundo fator é um app autenticador (TOTP): o código tem sempre 6
 * dígitos e troca a cada 30 segundos. */
export const TOTP_LENGTH = 6;

export const totpDigits = (code: string) => code.replace(/\D/g, "").slice(0, TOTP_LENGTH);

export const totpOk = (code: string) => new RegExp(`^\\d{${TOTP_LENGTH}}$`).test(code.trim());

/** O nível que a sessão tem e o que a conta exige. `aal1` é só a senha (ou o
 * provedor); `aal2` é com o código do app. */
export interface AssuranceLevel {
  currentLevel: string | null;
  nextLevel: string | null;
}

/** A conta tem o app cadastrado e a sessão ainda não passou por ele: o app
 * não abre nem entrega o token ao núcleo. */
export const needsSecondFactor = (level: AssuranceLevel | null | undefined) => level?.currentLevel === "aal1" && level?.nextLevel === "aal2";

/** A chave para digitar à mão, em blocos de 4 para não se perder nela. */
export const secretGroups = (secret: string) => secret.replace(/\s/g, "").match(/.{1,4}/g)?.join(" ") ?? "";
