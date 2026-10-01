import { toast } from "sonner";
import { isText, say } from "@/modules/i18n";

/** O aviso que qualquer módulo pode dar. Erro do núcleo vem como texto, e é
 * esse texto que aparece: ele já está escrito para quem lê a tela. */
export function notify(message: string, error = false) {
  if (error) toast.error(message, { duration: 6000 });
  else toast.success(message, { duration: 6000 });
}

export function reportError(error: unknown) {
  console.error(error);
  // O núcleo devolve a chave do i18n; o resto (login, rede) já vem como texto.
  notify(isText(error) ? say(error) : String(error), true);
}
