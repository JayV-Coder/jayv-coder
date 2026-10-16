import { describe, expect, it } from "vitest";
import { RenderGuard } from "./RenderGuard";

describe("RenderGuard", () => {
  it("segura o erro do desenho e tenta de novo quando a chave muda", () => {
    const caught = RenderGuard.getDerivedStateFromError(new URIError("URI malformed"));
    expect(caught.error?.message).toBe("URI malformed");
    expect(RenderGuard.getDerivedStateFromError("texto").error).toBeInstanceOf(Error);
    const props = { fallback: () => null, children: null };
    // A mesma tela: o erro fica até a pessoa tentar de novo.
    expect(RenderGuard.getDerivedStateFromProps({ ...props, resetKey: "chat" }, { error: new Error("x"), key: "chat" })).toBeNull();
    // Outra tela: desenha de novo.
    expect(RenderGuard.getDerivedStateFromProps({ ...props, resetKey: "settings" }, { error: new Error("x"), key: "chat" })).toEqual({ error: null, key: "settings" });
  });
});
