const KEY = "jayv.handledLinks";

/** O núcleo devolve o link com que o app foi aberto toda vez que a janela
 * carrega, e a troca de ambiente recarrega a janela: sem lembrar quais links
 * já foram tratados, o código de login (que só vale uma vez) seria trocado de
 * novo e o supabase-js reclamaria do verificador do PKCE. A sessionStorage
 * dura enquanto o app está aberto e sobrevive à recarga. */
export function firstTime(url: string, storage: Pick<Storage, "getItem" | "setItem"> | null = safeStorage()): boolean {
  if (!storage) return true;
  try {
    const seen: string[] = JSON.parse(storage.getItem(KEY) ?? "[]");
    if (seen.includes(url)) return false;
    storage.setItem(KEY, JSON.stringify([...seen, url].slice(-20)));
  } catch {
    // Sem armazenamento, trata o link como novo: é o comportamento de antes.
  }
  return true;
}

function safeStorage(): Storage | null {
  try {
    return sessionStorage;
  } catch {
    return null;
  }
}
