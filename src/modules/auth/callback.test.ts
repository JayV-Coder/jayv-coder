import { describe, expect, it } from "vitest";
import { readCallback } from "./callback";

describe("readCallback", () => {
  it("devolve o código do PKCE", () => expect(readCallback("jayv://auth/callback?code=abc")).toEqual({ code: "abc" }));
  it("traduz o erro conhecido da query", () =>
    expect(readCallback("jayv://auth/callback?error=server_error&error_code=identity_already_exists&error_description=Identity+is+already+linked"))
      .toEqual({ failure: { key: "auth.identityTaken" } }));
  it("lê o erro que vem no fragmento", () =>
    expect(readCallback("jayv://auth/callback#error=access_denied&error_code=otp_expired&error_description=Email+link+expired"))
      .toEqual({ failure: { key: "auth.badCode" } }));
  it("erro desconhecido fica com a descrição", () =>
    expect(readCallback("jayv://auth/callback?error=x&error_description=Algo")).toEqual({ failure: "Algo" }));
  it("ignora o que não é do app", () => {
    expect(readCallback("https://example.com/?code=abc")).toBeNull();
    expect(readCallback("não é url")).toBeNull();
  });
});
