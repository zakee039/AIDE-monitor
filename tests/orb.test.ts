import test from "node:test";
import assert from "node:assert/strict";
import { orbDisplay } from "../src/orb.ts";

const now = Date.parse("2026-10-06T00:00:00Z");
const recommendation = { state: "waiting" as const, accountId: "a", estimatedAvailableAt: "2026-10-06T04:10:00Z", reason: "quota_exhausted", coverage: { selected: 2, known: 1, unknown: 1, complete: false } };
test("4h10m recommendation survives missing, partial, zero and positive aggregate estimates", () => {
  for (const totalQuota of [undefined, { percent: null, partial: true, weeklyScalePercent: 15 }, { percent: 0, partial: true, weeklyScalePercent: 15 }, { percent: 0, partial: false, weeklyScalePercent: 15 }, { percent: 1, partial: false, weeklyScalePercent: 15 }]) {
    assert.deepEqual(orbDisplay({ recommendation, totalQuota }, now), { waiting: true, timeLines: ["4h", "10m"], text: "4h 10m" });
  }
});
test("expired or invalid reset is unknown, never a negative countdown", () => {
  for (const estimatedAvailableAt of [null, "invalid", "2026-10-05T23:59:59Z", new Date(now).toISOString()]) {
    assert.equal(orbDisplay({ recommendation: { ...recommendation, estimatedAvailableAt } }, now).text, "—");
  }
});
test("short waits and day waits use two lines", () => {
  assert.deepEqual(orbDisplay({ recommendation: { ...recommendation, estimatedAvailableAt: new Date(now + 10 * 60000).toISOString() } }, now).timeLines, ["0h", "10m"]);
  assert.deepEqual(orbDisplay({ recommendation: { ...recommendation, estimatedAvailableAt: new Date(now + (3 * 24 + 4) * 3600000).toISOString() } }, now).timeLines, ["3d", "4h"]);
});
test("available accounts keep the percentage and wholly unknown data keeps the dash", () => {
  assert.equal(orbDisplay({ recommendation: { ...recommendation, state: "now" }, totalQuota: { percent: 54.8, partial: false, weeklyScalePercent: 15 } }, now).text, "54%");
  assert.equal(orbDisplay({ recommendation: { ...recommendation, state: "unknown" } }, now).text, "—");
});
