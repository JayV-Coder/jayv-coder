import { create } from "zustand";
import { bus, commands, type SystemStatus } from "@/modules/core";
import { reportError } from "@/modules/feedback";

interface SystemState {
  status: SystemStatus | null;
}

export const useSystem = create<SystemState>(() => ({ status: null }));

export async function loadStatus() {
  try {
    useSystem.setState({ status: await commands.systemStatus() });
  } catch (error) {
    reportError(error);
  }
}

/** Os números do sistema mudam quando um pedido entra, quando a configuração
 * muda e quando a vista deles aparece. */
export function connectSystem() {
  const offs = [
    bus.on("prompt:sent", () => void loadStatus()),
    bus.on("settings:saved", () => void loadStatus()),
    bus.on("view:changed", ({ view }) => { if (view === "status") void loadStatus(); }),
  ];
  return () => offs.forEach((off) => off());
}
