import test from "node:test";
import assert from "node:assert/strict";
import { displayCount, selectedPools, poolOptions } from "../src/quota-pools.ts";
import { displayWindows } from "../src/monitor-display.ts";
import type { AccountSummary, AccountQuota } from "../contracts/hud-api.ts";

const google = { id: "g", providerId: "antigravity", displayName: "Google", selected: true, order: 0, support: "supported" } as AccountSummary;
test("pool count follows selected display entries and preserves old account defaults", () => {
  const accounts = [google, { ...google, id: "x", providerId: "grok" }, { ...google, id: "c", providerId: "codex" }];
  assert.deepEqual(selectedPools(google, {}), ["gemini3"]);
  assert.equal(displayCount(accounts, ["g", "x", "c"], { g: ["gemini3", "claude", "gemini4"] }), 5);
  assert.equal(displayCount(accounts, ["x", "c"], { g: ["gemini3", "claude", "gemini4"] }), 2);
  assert.equal(displayCount(accounts, ["g"], { g: [] }), 0);
  assert.deepEqual(poolOptions("grok").map(p => p.id), ["shared"]);
});
test("monitor renders the chosen pool and does not invent an hourly slot for Argon", () => {
  const q = { windows: [{ id: "gemini4-weekly", scope: "base", durationSeconds: 604800, applicability: "required", remainingPercent: 62 }] } as AccountQuota;
  const windows = displayWindows(q, "antigravity");
  assert.equal(windows.length, 1);
  assert.equal(windows[0].remainingPercent, 62);
  assert.equal(windows[0].durationSeconds, 604800);
});
