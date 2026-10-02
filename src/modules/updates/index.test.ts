import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));

const { checkForUpdate, closeUpdate, dismissUpdate, useUpdate } = await import("./index");

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

  it("o botão instala a versão já achada", async () => {
    const update = release("1.1.0");
    check.mockResolvedValue(update);
    await checkForUpdate();
    await checkForUpdate(true);
    expect(update.download).toHaveBeenCalled();
    expect(update.install).toHaveBeenCalled();
    expect(useUpdate.getState().phase).toBe("restarting");
  });
});
