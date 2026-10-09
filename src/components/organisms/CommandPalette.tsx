import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { openChatFind } from "@/modules/conversation";
import { fuzzyMatch, setPaletteOpen, shortcutFor, shortcutLabel, togglePalette, usePalette, type Shortcut } from "@/modules/commands";
import { environmentOrganization, useEnvironment } from "@/modules/environments";
import { useLocale, useLocales, useT } from "@/modules/i18n";
import { navigate, useNavigation } from "@/modules/navigation";
import { reportError } from "@/modules/feedback";
import { openOrganization, useOrganizations } from "@/modules/organizations";
import { isDirty, useSettings } from "@/modules/settings";
import { hasFeature, useEntitlements } from "@/modules/plans";
import { openStats } from "@/modules/usage";
import { createChat, findChat, findProject, leaveProject, nextWorkMode, setWorkMode, useWorkspace } from "@/modules/workspace";
import { Kbd } from "@/components/atoms";
import { cn } from "@/lib/utils";
import { paletteCommands, type Command } from "./paletteCommands";

/** Grifa as letras que a busca casou. */
function Marked({ text, positions }: { text: string; positions: number[] }) {
  if (!positions.length) return <>{text}</>;
  const marked = new Set(positions);
  const parts: ReactNode[] = [];
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    parts.push(marked.has(index)
      ? <mark key={index} className="bg-transparent text-foreground underline decoration-accent decoration-2 underline-offset-[3px]">{char}</mark>
      : char);
  }
  return <>{parts}</>;
}

/** O que cada atalho global faz. Atalho de projeto sem projeto aberto, ou de
 * chat sem chat aberto, não faz nada. No ambiente de uma organização, como no
 * menu lateral, Organizações e Sistema não existem e as Estatísticas são as
 * dela. */
function runShortcut(shortcut: Shortcut, projectId: string | null, chatId: string | null) {
  const orgId = environmentOrganization();
  switch (shortcut) {
    case "palette": togglePalette(); break;
    case "projects": leaveProject(); break;
    case "organizations": if (!orgId) navigate("organizations"); break;
    case "stats":
      if (orgId) void openOrganization(orgId, "stats").catch(reportError);
      else openStats({ kind: "global" });
      break;
    case "system": if (!orgId) navigate("status"); break;
    case "settings": navigate("settings"); break;
    case "newChat": if (projectId) void createChat(projectId); break;
    case "gate": if (projectId) navigate("gate"); break;
    case "find": if (chatId && useNavigation.getState().view === "chat" && hasFeature("conversationFind")) openChatFind(); break;
    case "workMode": {
      const chat = findChat(useWorkspace.getState().data, chatId);
      if (chat) void setWorkMode(chat.id, nextWorkMode(chat.workMode ?? "auto"));
      break;
    }
  }
}

/** A paleta de comandos (⌘K / Ctrl+K), como no Warp: tudo que tem botão
 * também tem nome aqui — ir a uma tela, abrir um projeto ou chat, trocar o
 * tema. A busca é difusa e o teclado manda: ↑/↓ movem, Enter roda, Esc fecha.
 *
 * Ela também escuta os atalhos globais (⌘1–4, ⌘, ⌘N, ⌘G), para que eles
 * funcionem em qualquer tela. */
export function CommandPalette() {
  const t = useT();
  const open = usePalette((state) => state.open);
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const organizations = useOrganizations((state) => state.list);
  const project = findProject(data, activeProjectId);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      const shortcut = shortcutFor(event);
      if (!shortcut) return;
      event.preventDefault();
      const { activeProjectId: projectId, activeChatId: chatId } = useWorkspace.getState();
      runShortcut(shortcut, projectId, chatId);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (open) { setQuery(""); setSelected(0); }
  }, [open]);

  const view = useNavigation((state) => state.view);
  const layout = useWorkspace((state) => state.layout);
  const locale = useLocale();
  const locales = useLocales();
  const invites = useOrganizations((state) => state.incoming);
  const openOrganizationId = useOrganizations((state) => state.openId);
  const environment = useEnvironment((state) => state.active);
  const settingsDirty = useSettings(isDirty);
  const rights = useEntitlements();

  const commands = useMemo<Command[]>(() => paletteCommands({
    t, data, project, activeChatId, view, layout, locale, locales, organizations, invites, openOrganizationId, environment, settingsDirty, rights,
  }), [t, project, data, activeChatId, view, layout, locale, locales, organizations, invites, openOrganizationId, environment, settingsDirty, rights]);

  const shown = useMemo(() => {
    const matched = commands
      .map((command) => ({ command, match: fuzzyMatch(query, command.label) }))
      .filter((item): item is { command: Command; match: NonNullable<ReturnType<typeof fuzzyMatch>> } => item.match !== null);
    // Com busca, o melhor casamento vem primeiro; sem busca, a ordem dos grupos.
    if (query.trim()) matched.sort((a, b) => b.match.score - a.match.score);
    return matched;
  }, [commands, query]);

  useEffect(() => setSelected(0), [query]);
  useEffect(() => {
    list.current?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const run = (command: Command | undefined) => {
    if (!command) return;
    setPaletteOpen(false);
    command.run();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") { event.preventDefault(); setSelected((index) => Math.min(index + 1, shown.length - 1)); }
    else if (event.key === "ArrowUp") { event.preventDefault(); setSelected((index) => Math.max(index - 1, 0)); }
    else if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); run(shown[selected]?.command); }
  };

  // Sem busca os itens ficam nos seus grupos; com busca, uma lista só, pela nota.
  const grouped = !query.trim();
  return (
    <DialogPrimitive.Root open={open} onOpenChange={setPaletteOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-overlay data-[state=open]:animate-in data-[state=open]:fade-in-0" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          className="fixed top-[14%] left-1/2 z-50 flex max-h-[70vh] w-[min(600px,calc(100%-2rem))] -translate-x-1/2 flex-col overflow-hidden rounded-lg border border-border bg-card font-mono text-card-foreground shadow-lg outline-none"
        >
          <DialogPrimitive.Title className="sr-only">{t("palette.title")}</DialogPrimitive.Title>
          <div className="flex items-center gap-2.5 border-b border-border px-3.5 py-2.5">
            <span aria-hidden="true" className="font-semibold text-go">❯</span>
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={onKeyDown}
              placeholder={t("palette.placeholder")}
              aria-label={t("palette.title")}
              role="combobox"
              aria-expanded="true"
              aria-controls="command-palette-list"
              aria-activedescendant={shown[selected] ? `command-${shown[selected].command.id}` : undefined}
              className="min-w-0 flex-1 bg-transparent text-sm caret-accent outline-none placeholder:text-muted-foreground"
            />
          </div>
          <div ref={list} id="command-palette-list" role="listbox" aria-label={t("palette.title")} className="min-h-0 flex-1 overflow-y-auto py-1">
            {shown.length === 0 && <p className="px-3.5 py-3 text-muted-foreground">{t("palette.empty")}</p>}
            {shown.map(({ command, match }, index) => {
              const heading = grouped && command.group !== shown[index - 1]?.command.group;
              return (
                <div key={command.id}>
                  {heading && <div className="px-3.5 pt-2.5 pb-1 text-caption tracking-wider text-muted-foreground uppercase">{t(command.group)}</div>}
                  <div
                    id={`command-${command.id}`}
                    role="option"
                    aria-selected={index === selected}
                    data-index={index}
                    onMouseMove={() => setSelected(index)}
                    onClick={() => run(command)}
                    className={cn(
                      "flex cursor-pointer items-center gap-3 px-3.5 py-1.5",
                      index === selected && "bg-secondary shadow-[inset_2px_0_0_var(--accent)]",
                    )}
                  >
                    <span className="min-w-0 truncate"><Marked text={command.label} positions={match.positions} /></span>
                    {command.hint && <span className="min-w-0 truncate text-caption text-muted-foreground">{command.hint}</span>}
                    {command.shortcut && (
                      <span className="ms-auto flex shrink-0 gap-0.5">{shortcutLabel(command.shortcut).map((key) => <Kbd key={key}>{key}</Kbd>)}</span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-1 border-t border-border px-3.5 py-1.5 text-caption text-muted-foreground">
            <span className="inline-flex items-center gap-1.5"><Kbd>↑↓</Kbd>{t("palette.hint.move")}</span>
            <span className="inline-flex items-center gap-1.5"><Kbd>↵</Kbd>{t("palette.hint.run")}</span>
            <span className="inline-flex items-center gap-1.5"><Kbd>Esc</Kbd>{t("palette.hint.close")}</span>
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
