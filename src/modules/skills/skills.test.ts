import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill } from "@/modules/core";

const skill = (name: string, enabled = true): Skill => ({ name, description: `${name} skill`, path: `/s/${name}`, enabled, installedAt: "" });
let installed: Skill[] = [];
const calls: string[] = [];
const commands = {
  getSkills: vi.fn(async () => installed),
  previewSkills: vi.fn(async () => [{ name: "one", description: "One." }, { name: "two", description: "Two." }]),
  removeSkill: vi.fn(async (name: string) => { calls.push(`remove ${name}`); installed = installed.filter((item) => item.name !== name); return installed; }),
  installSkillFolder: vi.fn(async (path: string) => { calls.push(`folder ${path}`); installed = [...installed.filter((item) => !["one", "two"].includes(item.name)), skill("one"), skill("two")]; return installed; }),
  installSkillText: vi.fn(async () => { calls.push("text"); return installed; }),
  installHubSkill: vi.fn(async (source: string, name: string) => { calls.push(`hub ${source}/${name}`); installed = [...installed, skill(name)]; return installed; }),
  setSkillEnabled: vi.fn(async (name: string, enabled: boolean) => { calls.push(`${enabled ? "on" : "off"} ${name}`); installed = installed.map((item) => (item.name === name ? { ...item, enabled } : item)); return installed; }),
};
vi.mock("@/modules/core", () => ({ commands }));
vi.mock("@/modules/feedback", () => ({ reportError: vi.fn() }));
vi.mock("@/modules/organizations", () => ({ pickFolder: vi.fn(async () => "/pick") }));
vi.mock("@/modules/i18n", () => ({ t: (key: string) => key }));

const { installFromSkillHub, installSkillFromFolder, isSkillsDirty, removeSkill, saveSkillChanges, setSkillEnabled, skillRows, useSkills } = await import("./index");

const rows = () => skillRows(useSkills.getState().skills ?? [], useSkills.getState().changes);

describe("skills wait for Save settings", () => {
  beforeEach(() => {
    installed = [skill("notes"), skill("old", false)];
    calls.length = 0;
    useSkills.setState({ skills: installed, changes: { removed: [], installs: [], enabled: {} } });
  });

  it("installing from a folder only lists the skills it brings", async () => {
    expect(await installSkillFromFolder()).toBe(true);
    expect(rows().map((row) => [row.name, row.pending])).toEqual([["notes", false], ["old", false], ["one", true], ["two", true]]);
    expect(commands.installSkillFolder).not.toHaveBeenCalled();
  });

  it("switching back to the saved value is not a change", () => {
    setSkillEnabled("notes", false);
    expect(isSkillsDirty(useSkills.getState())).toBe(true);
    setSkillEnabled("notes", true);
    expect(isSkillsDirty(useSkills.getState())).toBe(false);
  });

  it("removing a skill that was only going to be installed drops the install", () => {
    installFromSkillHub({ id: "a/b/hub", name: "hub", source: "a/b", installs: 1 });
    removeSkill("hub");
    expect(isSkillsDirty(useSkills.getState())).toBe(false);
  });

  it("Save applies removals, installs, left-out folder skills and switches in order", async () => {
    removeSkill("old");
    await installSkillFromFolder();
    removeSkill("two");
    setSkillEnabled("one", false);
    installFromSkillHub({ id: "a/b/hub", name: "hub", source: "a/b", installs: 1 });
    expect(rows().map((row) => [row.name, row.enabled])).toEqual([["hub", true], ["notes", true], ["one", false]]);
    expect(await saveSkillChanges()).toBe(true);
    expect(calls).toEqual(["remove old", "folder /pick", "remove two", "hub a/b/hub", "off one"]);
    expect(isSkillsDirty(useSkills.getState())).toBe(false);
    expect(useSkills.getState().skills?.map((item) => item.name)).toEqual(["notes", "one", "hub"]);
  });

  it("a failure keeps what is left pending", async () => {
    removeSkill("old");
    installFromSkillHub({ id: "a/b/hub", name: "hub", source: "a/b", installs: 1 });
    commands.installHubSkill.mockRejectedValueOnce(new Error("skills.hub.network"));
    expect(await saveSkillChanges()).toBe(false);
    const { changes } = useSkills.getState();
    expect(changes.removed).toEqual([]);
    expect(changes.installs.map((install) => install.kind)).toEqual(["hub"]);
  });
});
