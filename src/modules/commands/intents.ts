import { useEffect } from "react";
import { create } from "zustand";

/** O que a paleta pede a uma tela que já tem a janela ou o campo: abrir o
 * formulário do projeto novo, a memória do projeto, focar a busca de chats ou
 * abrir as notificações ou as permissões de comandos do chat. */
export type Intent = "newProject" | "projectNotes" | "searchChats" | "notifications" | "installSkill" | "searchSkillHub" | "commandPermissions";

/** Ações de funcionalidades que moram em `features/` e que a paleta chama sem
 * importá-las (a camada de baixo não depende de funcionalidade): cada uma se
 * registra ao ligar. */
export const featureActions: { reloadSystem?: () => void; copySystemReport?: () => Promise<void> } = {};

/** Quanto tempo o pedido vale: a tela dona pode demorar a aparecer depois da
 * navegação, mas um pedido que ninguém atendeu não deve abrir nada minutos depois. */
const INTENT_TTL_MS = 3000;

interface IntentState {
  pending: { intent: Intent; at: number } | null;
}

export const useIntent = create<IntentState>(() => ({ pending: null }));

/** Pede à tela dona do `intent` que o cumpra (a paleta navega antes). */
export function requestIntent(intent: Intent) {
  useIntent.setState({ pending: { intent, at: Date.now() } });
}

/** Quem tem a janela ou o campo cumpre o pedido da paleta quando ele chega. */
export function useIntentHandler(intent: Intent, handler: () => void) {
  const pending = useIntent((state) => state.pending);
  useEffect(() => {
    if (!pending || pending.intent !== intent) return;
    useIntent.setState({ pending: null });
    if (Date.now() - pending.at <= INTENT_TTL_MS) handler();
  }, [pending]); // eslint-disable-line react-hooks/exhaustive-deps
}
