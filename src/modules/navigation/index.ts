import { create } from "zustand";
import { bus, type View } from "@/modules/core";

interface NavigationState {
  view: View;
}

/** Qual tela está aberta. Trocar de tela é um aviso no barramento: cada módulo
 * decide sozinho o que precisa buscar quando a vista dele aparece. */
export const useNavigation = create<NavigationState>(() => ({ view: "projects" }));

export function navigate(view: View) {
  useNavigation.setState({ view });
  bus.emit("view:changed", { view });
}
