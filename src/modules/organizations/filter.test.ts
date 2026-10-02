import { describe, expect, it } from "vitest";
import { dashboardGateScope, dashboardScope } from "./filter";

const projects = ["p1", "p2"];

describe("dashboardScope", () => {
  it("counts every project of the organization when nothing is picked", () => {
    expect(dashboardScope({ projectId: null, chatId: null }, projects)).toEqual({ kind: "projects", id: projects });
  });

  it("narrows to one project and then to one of its chats", () => {
    expect(dashboardScope({ projectId: "p2", chatId: null }, projects)).toEqual({ kind: "project", id: "p2" });
    expect(dashboardScope({ projectId: "p2", chatId: "c9" }, projects)).toEqual({ kind: "chat", id: "c9" });
  });

  it("never widens to a project outside the organization", () => {
    expect(dashboardScope({ projectId: "other", chatId: "c1" }, projects)).toEqual({ kind: "projects", id: projects });
    expect(dashboardScope({ projectId: null, chatId: null }, [])).toEqual({ kind: "projects", id: [] });
  });
});

describe("dashboardGateScope", () => {
  it("reads every project, one project or one chat of it", () => {
    expect(dashboardGateScope({ projectId: null, chatId: null }, projects)).toEqual({ projectIds: projects, chatId: null });
    expect(dashboardGateScope({ projectId: "p1", chatId: null }, projects)).toEqual({ projectIds: ["p1"], chatId: null });
    expect(dashboardGateScope({ projectId: "p1", chatId: "c1" }, projects)).toEqual({ projectIds: ["p1"], chatId: "c1" });
  });

  it("drops a project outside the organization", () => {
    expect(dashboardGateScope({ projectId: "other", chatId: "c1" }, projects)).toEqual({ projectIds: projects, chatId: null });
  });
});
