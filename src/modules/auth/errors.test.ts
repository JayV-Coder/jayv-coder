import { describe, expect, it } from "vitest";
import { authFailure } from "./errors";

describe("authFailure", () => {
  it("traduz códigos conhecidos", () => {
    expect(authFailure({ code: "identity_already_exists", message: "x" })).toEqual({ key: "auth.identityTaken" });
    expect(authFailure({ code: "weak_password", message: "x" })).toEqual({ key: "auth.weakPassword" });
    expect(authFailure({ code: "invalid_credentials", message: "x" })).toEqual({ key: "auth.invalidCredentials" });
    expect(authFailure({ code: "wrong_password" })).toEqual({ key: "auth.wrongPassword" });
  });
  it("mantém a mensagem do resto", () => {
    expect(authFailure(new Error("rede caiu"))).toBe("rede caiu");
    expect(authFailure({ code: "nada", message: "outra" })).toBe("outra");
  });
});
