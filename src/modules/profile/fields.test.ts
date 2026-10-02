import { describe, expect, it } from "vitest";
import { fromRow, normalizeProfile, toRow, type AccountProfile } from "./fields";

const base: AccountProfile = { displayName: "Ana", fullName: null, sex: null, gender: null, genderCustom: null, pronouns: null, pronounsCustom: null, birthDate: null, country: null, timezone: null, role: null, company: null, completedAt: null };

describe("normalizeProfile", () => {
  it("apara e troca vazio por null", () => {
    expect(normalizeProfile({ ...base, displayName: "  Ana ", company: "  " })).toMatchObject({ displayName: "Ana", company: null });
  });
  it("limpa o texto livre quando a escolha não é a livre", () => {
    expect(normalizeProfile({ ...base, gender: "woman", genderCustom: "x" }).genderCustom).toBeNull();
    expect(normalizeProfile({ ...base, gender: "other", genderCustom: " agênero " }).genderCustom).toBe("agênero");
    expect(normalizeProfile({ ...base, pronouns: "she", pronounsCustom: "x" }).pronounsCustom).toBeNull();
  });
  it("país sempre em maiúsculas", () => expect(normalizeProfile({ ...base, country: "br" }).country).toBe("BR"));
});

describe("linhas", () => {
  it("ida e volta", () => expect(fromRow(toRow({ ...base, role: "qa" }) as never)).toMatchObject({ displayName: "Ana", role: "qa" }));
});
