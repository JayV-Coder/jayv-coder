// Os avisos do núcleo, de mentira: quem escuta fica guardado aqui, e o estúdio
// manda avisos (`emitCore`) como o núcleo mandaria.
type Handler = (event: { event: string; id: number; payload: unknown }) => void;

const handlers = new Map<string, Set<Handler>>();

export type UnlistenFn = () => void;

export async function listen(event: string, handler: Handler): Promise<UnlistenFn> {
  const set = handlers.get(event) ?? new Set<Handler>();
  set.add(handler);
  handlers.set(event, set);
  return () => set.delete(handler);
}

export async function emit(event: string, payload?: unknown) { emitCore(event, payload); }

export function emitCore(event: string, payload: unknown = null) {
  for (const handler of handlers.get(event) ?? []) handler({ event, id: 0, payload });
}
