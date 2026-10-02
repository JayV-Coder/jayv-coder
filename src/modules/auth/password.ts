/** As regras da senha nova. O servidor confere as mesmas; esta cópia é só
 * para a lista que marca enquanto a pessoa digita. */
export type PasswordRule = "length" | "lower" | "upper" | "digit" | "symbol";
export const PASSWORD_RULES: PasswordRule[] = ["length", "lower", "upper", "digit", "symbol"];
export const PASSWORD_MIN = 8;

export function passwordRules(password: string): Record<PasswordRule, boolean> {
  return {
    length: [...password].length >= PASSWORD_MIN,
    lower: /\p{Ll}/u.test(password),
    upper: /\p{Lu}/u.test(password),
    digit: /\p{Nd}/u.test(password),
    symbol: /[^\p{L}\p{Nd}]/u.test(password),
  };
}

export const passwordOk = (password: string) => Object.values(passwordRules(password)).every(Boolean);
