import { describe, expect, it } from "vitest";
import { canUnlink } from "./identities";

describe("canUnlink", () => {
  it("recusa a última forma de entrar", () => expect(canUnlink(["github"], false, "github")).toBe(false));
  it("aceita com senha", () => expect(canUnlink(["github"], true, "github")).toBe(true));
  it("aceita com outro provedor", () => expect(canUnlink(["github", "gitlab"], false, "github")).toBe(true));
  it("a identidade email conta como forma de entrar", () => expect(canUnlink(["email", "gitlab"], false, "gitlab")).toBe(true));
  it("não desvincula o que não está vinculado", () => expect(canUnlink(["email"], true, "github")).toBe(false));
});
