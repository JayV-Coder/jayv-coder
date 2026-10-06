import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));

const { checkForUpdate, closeUpdate, dismissUpdate, installUpdate, useUpdate } = await import("./index");

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

describe("checkedAt", () => {
  it("records when a check answered", async () => {
    check.mockReset();
    check.mockResolvedValue(null);
    useUpdate.setState({ phase: "idle", checkedAt: null });
    await checkForUpdate();
    expect(useUpdate.getState().checkedAt).not.toBeNull();
  });
});

describe("updateAtLaunch", () => {
  const fresh = async () => {
    vi.resetModules();
    return import("./index");
  };

  it("installs the version found at startup by itself", async () => {
    const update = release("1.1.0");
    check.mockReset();
    check.mockResolvedValue(update);
    const { updateAtLaunch, useUpdate: state } = await fresh();
    state.setState({ installOnLaunch: true });
    await updateAtLaunch();
    expect(update.download).toHaveBeenCalled();
    expect(update.install).toHaveBeenCalled();
    expect(state.getState()).toMatchObject({ launching: false, phase: "restarting" });
  });

  it("with the option off, only announces it", async () => {
    const update = release("1.1.0");
    check.mockReset();
    check.mockResolvedValue(update);
    const { updateAtLaunch, useUpdate: state } = await fresh();
    state.setState({ installOnLaunch: false });
    await updateAtLaunch();
    expect(update.download).not.toHaveBeenCalled();
    expect(state.getState()).toMatchObject({ phase: "available", next: "1.1.0", launching: false });
  });

  it("without a network, the app opens after the failed check", async () => {
    check.mockReset();
    check.mockRejectedValue(new Error("offline"));
    const { updateAtLaunch, useUpdate: state } = await fresh();
    await updateAtLaunch();
    expect(state.getState()).toMatchObject({ launching: false, phase: "idle", error: null });
  });

  it("skip opens the app at once and a late answer becomes the notice", async () => {
    const update = release("1.1.0");
    let answer: (value: unknown) => void = () => undefined;
    check.mockReset();
    check.mockReturnValue(new Promise((resolve) => { answer = resolve; }));
    const { updateAtLaunch, skipLaunchUpdate, useUpdate: state } = await fresh();
    state.setState({ installOnLaunch: true });
    const running = updateAtLaunch();
    expect(state.getState().launching).toBe(true);
    skipLaunchUpdate();
    await running;
    expect(state.getState().launching).toBe(false);
    answer(update);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(update.download).not.toHaveBeenCalled();
    expect(state.getState()).toMatchObject({ phase: "available", next: "1.1.0" });
  });
});
