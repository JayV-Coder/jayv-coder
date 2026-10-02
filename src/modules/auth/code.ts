/** O código que o Supabase manda por e-mail tem de 6 a 10 dígitos, conforme o
 * "Email OTP Length" do projeto (projetos novos já saem com 8). A tela não
 * pode supor 6: cortar o código colado o torna inválido, e o servidor responde
 * com a mesma mensagem do código expirado. */
export const CODE_MIN = 6;
export const CODE_MAX = 10;

export const codeDigits = (code: string) => code.replace(/\D/g, "").slice(0, CODE_MAX);

export function codeOk(code: string) {
  const digits = code.trim();
  return /^\d+$/.test(digits) && digits.length >= CODE_MIN && digits.length <= CODE_MAX;
}
