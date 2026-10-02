import { describe, expect, it } from "vitest";
import { daysInMonth, fromRow, joinDate, normalizeProfile, splitDate, toRow, usernameOk, type AccountProfile } from "./fields";

const base: AccountProfile = { displayName: "Ana", username: "ana", sex: null, gender: null, genderCustom: null, pronouns: null, pronounsCustom: null, birthDate: null, country: null, timezone: null, role: null, company: null, completedAt: null, usernameSetAt: null };

describe("normalizeProfile", () => {
  it("apara e troca vazio por null", () => {
    expect(normalizeProfile({ ...base, displayName: "  Ana ", company: "  " })).toMatchObject({ displayName: "Ana", company: null });
  });
  it("limpa o texto livre quando a escolha não é a livre", () => {
    expect(normalizeProfile({ ...base, gender: "woman", genderCustom: "x" }).genderCustom).toBeNull();
    expect(normalizeProfile({ ...base, gender: "other", genderCustom: " agênero " }).genderCustom).toBe("agênero");
    expect(normalizeProfile({ ...base, pronouns: "she", pronounsCustom: "x" }).pronounsCustom).toBeNull();
  });
  it("nome de usuário em minúsculas e sem espaços", () => expect(normalizeProfile({ ...base, username: " Ana-Dev " }).username).toBe("ana-dev"));
  it("país sempre em maiúsculas", () => expect(normalizeProfile({ ...base, country: "br" }).country).toBe("BR"));
});

describe("linhas", () => {
  it("ida e volta", () => expect(fromRow(toRow({ ...base, role: "qa" }) as never)).toMatchObject({ displayName: "Ana", role: "qa" }));
  it("a linha para gravar não mexe no nome fixado", () => expect(toRow({ ...base, usernameSetAt: "2026-01-01T00:00:00Z" })).not.toHaveProperty("username_set_at"));
  it("o nome fixado volta do banco", () => {
    expect(fromRow({ ...toRow(base), username_set_at: "2026-01-01T00:00:00Z" }).usernameSetAt).toBe("2026-01-01T00:00:00Z");
    expect(fromRow(toRow(base)).usernameSetAt).toBeNull();
  });
});

describe("usernameOk", () => {
  it("aceita o formato", () => {
    for (const name of ["ana", "ana-dev", "a_1", "x".repeat(30)]) expect(usernameOk(name)).toBe(true);
  });
  it("recusa o resto", () => {
    for (const name of ["an", "x".repeat(31), "Ana", "-ana", "ana_", "ana dev", "joão"]) expect(usernameOk(name)).toBe(false);
  });
});

describe("data em partes", () => {
  it("dias por mês, com bissexto", () => {
    expect(daysInMonth(2024, 2)).toBe(29);
    expect(daysInMonth(2023, 2)).toBe(28);
    expect(daysInMonth(1900, 2)).toBe(28);
    expect(daysInMonth(2000, 2)).toBe(29);
    expect(daysInMonth(2023, 4)).toBe(30);
    expect(daysInMonth(null, 2)).toBe(29);
    expect(daysInMonth(2023, null)).toBe(31);
  });
  it("ida e volta", () => {
    expect(splitDate("1990-03-07")).toEqual({ year: 1990, month: 3, day: 7 });
    expect(splitDate(null)).toEqual({ year: null, month: null, day: null });
    expect(joinDate({ year: 1990, month: 3, day: 7 })).toBe("1990-03-07");
  });
  it("só grava com as três partes", () => expect(joinDate({ year: 1990, month: null, day: 7 })).toBeNull());
});
