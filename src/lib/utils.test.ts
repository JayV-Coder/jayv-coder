import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("mantém a cor do texto ao lado da escala de texto", () => {
    expect(cn("text-primary-foreground", "text-caption")).toBe("text-primary-foreground text-caption");
  });

  it("a escala de texto troca o tamanho anterior", () => {
    expect(cn("text-sm", "text-caption")).toBe("text-caption");
  });
});
