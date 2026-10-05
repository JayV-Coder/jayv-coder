import { createRequire } from "node:module";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));

const { RELEASES } = await import("./index");
const require = createRequire(import.meta.url);
const { problems, publication } = require("../../../.github/scripts/manual.cjs") as {
  problems: () => string[];
  publication: () => Record<string, string>;
};

describe("docs/manual", () => {
  it("toda funcionalidade tem a sua página, completa, e todo recurso de plano está documentado", () => {
    expect(problems()).toEqual([]);
  });

  it("o que vai para o repositório de releases: uma página por funcionalidade, os comandos e o changelog", () => {
    const files = publication();
    const index = JSON.parse(files["manual/index.json"]);
    for (const id of index.features) expect(Object.keys(files)).toContain(`manual/features/${id}.json`);
    expect(JSON.parse(files["manual/commands.json"]).commands.length).toBeGreaterThan(0);
    const changelog = JSON.parse(files["changelog.json"]);
    expect(changelog.releases.map((release: { version: string }) => release.version)).toEqual(RELEASES.map((release) => release.version));
    expect(changelog.releases[0].items.every((item: { title: string | null }) => !!item.title)).toBe(true);
  });
});
