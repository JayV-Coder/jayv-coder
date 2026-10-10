import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

const storage = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => void storage.set(key, value), removeItem: (key: string) => void storage.delete(key) });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const { TOURS, MANUAL, endTour, nextStep, paginate, previousStep, setStepPages, skipAllTours, startTour, stepText, textWidth, tourForView, useTutorial, resetTours, PAGE_BUDGET } = await import("./index");

const root = join(__dirname, "../../..");

describe("tours", () => {
  it("every feature of the manual has a step in some tutorial", () => {
    const ids = readdirSync(join(root, "docs/manual/features")).map((name) => name.replace(/\.json$/, ""));
    const covered = new Set(TOURS.flatMap((tour) => tour.steps.map((step) => step.feature)));
    expect(ids.filter((id) => !covered.has(id))).toEqual([]);
  });

  it("every step points to a feature that exists", () => {
    for (const step of TOURS.flatMap((tour) => tour.steps)) expect(MANUAL[step.feature], step.feature).toBeDefined();
  });

  it("every highlighted target exists in the screen code", () => {
    const sources = ["components/organisms/Sidebar.tsx", "components/organisms/EnvironmentSwitch.tsx", "components/organisms/NotificationBell.tsx", "components/organisms/Composer.tsx", "components/organisms/GrantsPicker.tsx", "components/organisms/AppHeader.tsx", "components/organisms/GeneralChatsColumn.tsx", "components/pages/SettingsPage.tsx"]
      .map((file) => readFileSync(join(root, "src", file), "utf8")).join("\n");
    for (const target of TOURS.flatMap((tour) => tour.steps.flatMap((step) => (step.target ? [step.target] : [])))) {
      expect(sources, target).toContain(`data-tour="${target}"`);
    }
  });

  it("each screen has at most one tutorial of its own", () => {
    const views = TOURS.map((tour) => tour.view);
    expect(new Set(views).size).toBe(views.length);
    expect(tourForView("chat")?.id).toBe("chat");
  });
});

describe("tutorial store", () => {
  beforeEach(() => { storage.clear(); useTutorial.setState({ active: null, seen: [], auto: true, pages: 1 }); });

  it("walks the steps and marks the tour as seen at the end", () => {
    const tour = TOURS[0];
    startTour(tour.id);
    nextStep();
    expect(useTutorial.getState().active).toEqual({ tour: tour.id, step: 1, page: 0 });
    previousStep();
    expect(useTutorial.getState().active?.step).toBe(0);
    for (let i = 0; i < tour.steps.length; i++) nextStep();
    expect(useTutorial.getState().active).toBeNull();
    expect(useTutorial.getState().seen).toContain(tour.id);
    expect(JSON.parse(storage.get("jayv.tutorial") ?? "{}").seen).toContain(tour.id);
  });

  it("closing early also counts as seen; skip all stops the automatic ones", () => {
    startTour("chat");
    endTour();
    expect(useTutorial.getState().seen).toEqual(["chat"]);
    skipAllTours();
    expect(useTutorial.getState()).toMatchObject({ auto: false, active: null });
    expect(useTutorial.getState().seen.length).toBe(TOURS.length);
    resetTours();
    expect(useTutorial.getState().seen).toEqual([]);
  });

  it("text comes from the translation, falling back to the manual in English", () => {
    expect(stepText("workModes", "title", { "docs.workModes.title": "Modos" })).toBe("Modos");
    expect(stepText("workModes", "title", {})).toBe(MANUAL.workModes.title);
  });

  it("walks the parts of a long step before the next step, and back into the last part", () => {
    const tour = TOURS[0];
    startTour(tour.id);
    setStepPages(3);
    nextStep();
    nextStep();
    expect(useTutorial.getState().active).toEqual({ tour: tour.id, step: 0, page: 2 });
    nextStep();
    expect(useTutorial.getState().active).toEqual({ tour: tour.id, step: 1, page: 0 });
    previousStep();
    expect(useTutorial.getState().active).toEqual({ tour: tour.id, step: 0, page: -1 });
    setStepPages(3);
    expect(useTutorial.getState().active).toEqual({ tour: tour.id, step: 0, page: 2 });
    previousStep();
    expect(useTutorial.getState().active?.page).toBe(1);
  });
});

describe("parts of a step", () => {
  const features = Object.values(MANUAL);

  it("keeps a short step in one part", () => {
    expect(paginate(MANUAL.secretRedaction.summary, MANUAL.secretRedaction.usage, "en")).toHaveLength(1);
  });

  it("splits the long steps without losing or cutting any sentence", () => {
    for (const feature of features) {
      const pages = paginate(feature.summary, feature.usage, "en");
      const joined = (kind: "summary" | "usage") => pages.flat().filter((part) => part.kind === kind).map((part) => part.text).join(" ");
      expect(joined("summary").replace(/\s+/g, " "), feature.id).toBe(feature.summary.trim().replace(/\s+/g, " "));
      expect(joined("usage").replace(/\s+/g, " "), feature.id).toBe(feature.usage.trim().replace(/\s+/g, " "));
    }
    expect(paginate(MANUAL.environments.summary, MANUAL.environments.usage, "en").length).toBeGreaterThan(2);
  });

  it("shares the text evenly: no part is a leftover sentence next to a full one", () => {
    for (const feature of features) {
      const sizes = paginate(feature.summary, feature.usage, "en").map((page) => page.reduce((sum, part) => sum + textWidth(part.text), 0));
      if (sizes.length < 2) continue;
      expect(Math.max(...sizes), feature.id).toBeLessThanOrEqual(PAGE_BUDGET * 1.25);
      expect(Math.min(...sizes), feature.id).toBeGreaterThan(PAGE_BUDGET / 4);
    }
  });

  it("counts ideograms as wide, so Chinese and Japanese parts hold fewer characters", () => {
    expect(textWidth("环境")).toBe(4);
    expect(textWidth("ab")).toBe(2);
  });
});
