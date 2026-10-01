import { create } from "zustand";
import { commands, onCore, type ConnectionStatus } from "@/modules/core/bridge";

export const useConnection = create<ConnectionStatus>(() => ({ link: "signedOut", pending: 0, failed: 0 }));

async function refresh() {
  const status = await commands.connectionStatus().catch(() => null);
  if (status) useConnection.setState(status);
}

/** O estado da conexão vem pelo evento; a contagem da fila, por consulta —
 * ela muda a cada escrita, e um evento por escrita seria barulho. */
export function connectConnection() {
  void refresh();
  const timer = setInterval(() => void refresh(), 5000);
  const off = onCore("link-changed", ({ link }) => {
    useConnection.setState({ link });
    void refresh();
  });
  return () => {
    clearInterval(timer);
    void off.then((unlisten) => unlisten());
  };
}
