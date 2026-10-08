/** A troca de ambiente recarrega a janela. O que carrega na frente dela (a
 * consulta de atualização de quando o app abre) não tem o que fazer de novo: o
 * app já estava aberto e atualizado. A marca fica na sessão da janela e vale
 * para uma recarga só. */
const KEY = "jayv.environment.switching";

export function markSwitching() {
  try { sessionStorage.setItem(KEY, "1"); } catch { /* sem armazenamento: a janela abre como de costume */ }
}

let resumed: boolean | undefined;

/** Esta janela foi recarregada por uma troca de ambiente? Lido e apagado uma
 * vez; as demais chamadas repetem a resposta. */
export function resumedFromSwitch(): boolean {
  if (resumed !== undefined) return resumed;
  try {
    resumed = sessionStorage.getItem(KEY) === "1";
    sessionStorage.removeItem(KEY);
  } catch { resumed = false; }
  return resumed;
}
