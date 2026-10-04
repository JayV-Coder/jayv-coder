import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { showChanges } from "@/modules/changelog";
import { reportError } from "@/modules/feedback";
import { fuzzyMatch, setPaletteOpen, shortcutFor, shortcutLabel, togglePalette, usePalette, type Shortcut } from "@/modules/commands";
import { useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { openOrganization, useOrganizations } from "@/modules/organizations";
import { setThemePreference, THEME_PREFERENCES } from "@/modules/theme";
import { openStats } from "@/modules/usage";
import { WORK_MODES } from "@/modules/core";
import { chatTitle, chatsOf, createChat, findChat, findProject, leaveProject, nextWorkMode, openChat, openProject, recentChats, setWorkMode, useWorkspace } from "@/modules/workspace";
import { Kbd } from "@/components/atoms";
import { cn } from "@/lib/utils";

interface Command {
  id: string;
  group: Key;
  label: string;
  /** Texto apagado ao lado do nome: o caminho do projeto, por exemplo. */
  hint?: string;
  shortcut?: Shortcut;
  run: () => void;
}

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
 * chat sem chat aberto, não faz nada. */
function runShortcut(shortcut: Shortcut, projectId: string | null, chatId: string | null) {
  switch (shortcut) {
    case "palette": togglePalette(); break;
    case "projects": leaveProject(); break;
    case "organizations": navigate("organizations"); break;
    case "stats": openStats({ kind: "global" }); break;
    case "system": navigate("status"); break;
    case "settings": navigate("settings"); break;
    case "newChat": if (projectId) void createChat(projectId); break;
    case "gate": if (projectId) navigate("gate"); break;
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

  const commands = useMemo<Command[]>(() => {
    const all: Command[] = [];
    if (project) {
      all.push(
        { id: "new-chat", group: "palette.group.project", label: t("common.newChat"), hint: project.name, shortcut: "newChat", run: () => void createChat(project.id) },
        { id: "chats", group: "palette.group.project", label: t("nav.chats"), hint: project.name, run: () => navigate("chats") },
        { id: "gate", group: "palette.group.project", label: t("nav.gate"), hint: project.name, shortcut: "gate", run: () => navigate("gate") },
        { id: "project-stats", group: "palette.group.project", label: t("nav.stats"), hint: project.name, run: () => openStats({ kind: "project", id: project.id }) },
      );
      const chat = findChat(data, activeChatId);
      if (chat) {
        for (const mode of WORK_MODES) {
          if (mode === (chat.workMode ?? "auto")) continue;
          all.push({ id: `mode-${mode}`, group: "palette.group.project", label: t("palette.mode", { name: t(`mode.${mode}`) }), hint: t(`mode.${mode}.hint`), run: () => void setWorkMode(chat.id, mode) });
        }
      }
      for (const chat of recentChats(chatsOf(data, project.id), activeChatId)) {
        all.push({ id: `chat-${chat.id}`, group: "palette.group.chats", label: chatTitle(chat), run: () => openChat(chat.id) });
      }
    }
    all.push(
      { id: "projects", group: "palette.group.navigate", label: t("nav.projects"), shortcut: "projects", run: leaveProject },
      { id: "organizations", group: "palette.group.navigate", label: t("nav.organizations"), shortcut: "organizations", run: () => navigate("organizations") },
      { id: "stats", group: "palette.group.navigate", label: t("nav.stats"), shortcut: "stats", run: () => openStats({ kind: "global" }) },
      { id: "system", group: "palette.group.navigate", label: t("nav.system"), shortcut: "system", run: () => navigate("status") },
      { id: "settings", group: "palette.group.navigate", label: t("nav.settings"), shortcut: "settings", run: () => navigate("settings") },
      { id: "profile", group: "palette.group.navigate", label: t("nav.profile"), run: () => navigate("profile") },
    );
    for (const item of data.projects) {
      if (item.id === project?.id) continue;
      all.push({ id: `project-${item.id}`, group: "palette.group.projects", label: item.name, hint: item.rootPath ?? undefined, run: () => openProject(item.id) });
    }
    for (const organization of organizations) {
      all.push({ id: `org-${organization.id}`, group: "palette.group.organizations", label: organization.name, run: () => void openOrganization(organization.id).catch(reportError) });
    }
    for (const preference of THEME_PREFERENCES) {
      all.push({ id: `theme-${preference}`, group: "palette.group.appearance", label: t("palette.theme", { name: t(`theme.${preference}`) }), run: () => setThemePreference(preference) });
    }
    all.push({ id: "whats-new", group: "palette.group.help", label: t("palette.whatsNew"), run: () => void showChanges() });
    return all;
  }, [t, project, data, activeChatId, organizations]);

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
