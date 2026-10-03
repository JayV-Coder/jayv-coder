import { describe, expect, it, vi } from "vitest";
import { en } from "@/modules/i18n/messages/en";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));

const { RELEASES, compareVersions, releaseFromNotes, releasesUpTo, unseenReleases } = await import("./index");

const release = (version: string) => ({ version, date: "2026-10-02", items: [] });
const history = [release("1.2.0"), release("1.1.1"), release("1.1.0"), release("1.0.0")];
const versions = (list: { version: string }[]) => list.map((item) => item.version);

describe("compareVersions", () => {
  it("compara número a número, não como texto", () => {
    expect(compareVersions("0.10.0", "0.9.9")).toBe(1);
    expect(compareVersions("0.32.1", "0.33.0")).toBe(-1);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("1.0.0-beta", "1.0.0")).toBe(0);
  });
});

describe("unseenReleases", () => {
  it("depois de uma atualização mostra as versões entre a vista e a instalada", () => {
    expect(versions(unseenReleases("1.2.0", "1.0.0", history))).toEqual(["1.2.0", "1.1.1", "1.1.0"]);
  });

  it("sem versão vista mostra só a instalada", () => {
    expect(versions(unseenReleases("1.1.1", null, history))).toEqual(["1.1.1"]);
  });

  it("um changelog mais novo que o app não mostra o que ainda não chegou", () => {
    expect(versions(releasesUpTo("1.1.0", history))).toEqual(["1.1.0", "1.0.0"]);
    expect(unseenReleases("1.1.1", "1.1.1", history)).toEqual([]);
  });
});

describe("RELEASES", () => {
  it("vem da mais nova para a mais antiga, sem versão repetida", () => {
    for (let index = 1; index < RELEASES.length; index++) {
      expect(compareVersions(RELEASES[index - 1].version, RELEASES[index].version)).toBe(1);
    }
  });

  it("a versão do app tem entrada no changelog", async () => {
    const { version } = await import("../../../package.json");
    expect(RELEASES[0].version).toBe(version);
  });

  it("todo item tem título e detalhe em inglês", () => {
    for (const { items } of RELEASES) {
      expect(items.length).toBeGreaterThan(0);
      for (const { id } of items) {
        expect(Object.keys(en)).toContain(`whatsNew.item.${id}.title`);
        expect(Object.keys(en)).toContain(`whatsNew.item.${id}.detail`);
      }
    }
  });
});

describe("releaseFromNotes", () => {
  it("lê as novidades que o release.yml põe nas notas, com o inglês de cada item", async () => {
    const { createRequire } = await import("node:module");
    const { whatsNewComment } = createRequire(import.meta.url)("../../../.github/scripts/whats-new.cjs");
    const notes = `Instaladores do JayV.\n\n<!-- source: abc -->\n${whatsNewComment()}`;
    const found = releaseFromNotes(notes);
    expect(found?.version).toBe(RELEASES[0].version);
    expect(found?.items.map((item) => item.id)).toEqual(RELEASES[0].items.map((item) => item.id));
    const first = RELEASES[0].items[0];
    expect(found?.items[0].title).toBe(en[`whatsNew.item.${first.id}.title` as keyof typeof en]);
  });

  it("notas sem o trecho, ou com ele quebrado, não dão nada", () => {
    expect(releaseFromNotes("- v0.1: coisas")).toBeNull();
    expect(releaseFromNotes("<!-- whats-new: bm9wZQ== -->")).toBeNull();
    expect(releaseFromNotes(null)).toBeNull();
  });
});
