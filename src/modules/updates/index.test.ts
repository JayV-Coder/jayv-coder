import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));

const { checkForUpdate, closeUpdate, dismissUpdate, installUpdate, setUpdateInterval, storedInterval, useUpdate } = await import("./index");

const release = (version: string) => ({
  currentVersion: "1.0.0", version, body: "notes", date: null,
  download: vi.fn(async () => undefined), install: vi.fn(async () => undefined), close: vi.fn(async () => undefined),
});

describe("checkForUpdate", () => {
  beforeEach(() => {
    check.mockReset();
    useUpdate.setState({ phase: "idle", open: false, next: null, dismissed: null });
  });

  it("a consulta silenciosa só avisa: não abre a janela nem instala", async () => {
    const update = release("1.1.0");
    check.mockResolvedValue(update);
    await checkForUpdate();
    expect(useUpdate.getState()).toMatchObject({ phase: "available", open: false, next: "1.1.0" });
    expect(update.download).not.toHaveBeenCalled();
  });

  it("fechar a janela mantém a versão esperando; o aviso some só para ela", async () => {
    check.mockResolvedValue(release("1.1.0"));
    await checkForUpdate();
    useUpdate.setState({ open: true });
    closeUpdate();
    expect(useUpdate.getState()).toMatchObject({ phase: "available", open: false });
    dismissUpdate();
    expect(useUpdate.getState().dismissed).toBe("1.1.0");
  });

  it("sem rede, a consulta silenciosa fica calada e não perde a versão achada", async () => {
    check.mockResolvedValueOnce(release("1.1.0")).mockRejectedValueOnce(new Error("offline"));
    await checkForUpdate();
    await checkForUpdate();
    expect(useUpdate.getState()).toMatchObject({ phase: "available", next: "1.1.0", error: null });
  });

  it("o botão de procurar mostra a versão achada e espera a escolha", async () => {
    const update = release("1.1.0");
    check.mockResolvedValue(update);
    await checkForUpdate(true);
    expect(useUpdate.getState()).toMatchObject({ phase: "available", open: true, next: "1.1.0" });
    expect(update.download).not.toHaveBeenCalled();
    closeUpdate();
    expect(useUpdate.getState()).toMatchObject({ phase: "available", open: false });
    expect(update.download).not.toHaveBeenCalled();
  });

  it("a versão já achada também só reabre a janela, sem instalar", async () => {
    const update = release("1.1.0");
    check.mockResolvedValue(update);
    await checkForUpdate();
    await checkForUpdate(true);
    expect(useUpdate.getState()).toMatchObject({ phase: "available", open: true });
    expect(update.download).not.toHaveBeenCalled();
  });

  it("atualizar agora instala a versão já achada", async () => {
    const update = release("1.1.0");
    check.mockResolvedValue(update);
    await checkForUpdate();
    await checkForUpdate(true);
    await installUpdate();
    expect(update.download).toHaveBeenCalled();
    expect(update.install).toHaveBeenCalled();
    expect(useUpdate.getState().phase).toBe("restarting");
  });
});

describe("setUpdateInterval", () => {
  it("keeps only the offered intervals and remembers the choice", () => {
    const saved = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (key: string) => saved.get(key) ?? null, setItem: (key: string, value: string) => saved.set(key, value) });
    setUpdateInterval(60);
    expect(useUpdate.getState().every).toBe(60);
    expect(storedInterval()).toBe(60);
    setUpdateInterval(7);
    expect(useUpdate.getState().every).toBe(60);
    vi.unstubAllGlobals();
  });

  it("records when a check answered", async () => {
    check.mockReset();
    check.mockResolvedValue(null);
    useUpdate.setState({ phase: "idle", checkedAt: null });
    await checkForUpdate();
    expect(useUpdate.getState().checkedAt).not.toBeNull();
  });
});
