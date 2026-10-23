import test from "node:test";
import assert from "node:assert/strict";
import { effectiveQuotaWindows, historicalWindow, historicalProvider } from "../src/quota-display.ts";
import type { AccountQuota, Snapshot } from "../contracts/hud-api.ts";

const now = Date.parse("2026-10-23T00:00:00Z");
const at = (hours: number) => new Date(now + hours * 3600000).toISOString();
function quota(five: number | null, week: number | null, ratio = 0.15): AccountQuota {
  return { accountId: "a", origin: "network", freshness: "fresh", status: "ok", lastSuccessAt: at(-1), lastAttemptAt: at(-1), observedAt: at(-1), validUntil: at(1), providerAllowed: true, baseCoverageComplete: true, blockingReason: "none", error: null, fiveHourWeeklyRatio: ratio,
    windows: [five, week].map((value, i) => ({ id: String(i), scope: "base", kind: i ? "secondary" : "primary", label: i ? "7d" : "5h", durationSeconds: i ? 604800 : 18000, applicability: "required", measurement: value == null ? "unknown" : "percent", remainingPercent: value, exhausted: value == null ? null : value === 0, resetsAt: at(i ? 120 : 4) })) };
}
test("5h stays in its own units, with full quota at 100%, and is capped by the weekly budget", () => {
  for (const [five, week, ratio, expected] of [[100, 100, .15, 100], [85, 90, .15, 85], [100, 0, .15, 0], [98, 4, .15, 4 / .15], [100, 55, .5, 100], [100, 10, .25, 40], [80, 90, .1, 80], [80, 90, .2, 80]]) {
    const raw = quota(five, week, ratio);
    const result = effectiveQuotaWindows(raw);
    assert.equal(result[0].remainingPercent, expected);
    assert.equal(result[1].remainingPercent, week);
    assert.equal(raw.windows[0].remainingPercent, five);
    assert.equal(result[0].resetsAt, raw.windows[0].resetsAt);
  }
});
test("weekly-only, monthly and missing measurements do not manufacture a 5h balance", () => {
  const weekly = quota(85, 70); weekly.windows.shift();
  assert.deepEqual(effectiveQuotaWindows(weekly), weekly.windows);
  weekly.windows[0].durationSeconds = 31 * 86400;
  assert.deepEqual(effectiveQuotaWindows(weekly), weekly.windows);
  assert.equal(effectiveQuotaWindows(quota(85, null))[0].remainingPercent, null);
  assert.equal(effectiveQuotaWindows(quota(null, 0))[0].remainingPercent, 0);
});
test("only historical measurements fade; a retained week also dims the derived 5h balance", () => {
  const raw = quota(85, 70);
  raw.windows[1].retainedAt = at(-10);
  const result = effectiveQuotaWindows(raw);
  assert.ok(result.every(w => historicalWindow(raw, w, now)));
  assert.equal(historicalWindow(raw, raw.windows[0], now), false);
  const snapshot = { accounts: [{ id: "a", providerId: "codex" }], quotas: [raw] } as Snapshot;
  assert.equal(historicalProvider(snapshot, "chatgpt", now), true);
  assert.equal(historicalProvider(snapshot, "claude", now), false);
  raw.windows[1].retainedAt = null;
  assert.equal(historicalProvider(snapshot, "chatgpt", now), false);
  raw.status = "error";
  assert.ok(raw.windows.every(w => historicalWindow(raw, w, now)));
  raw.status = "ok"; raw.validUntil = at(-1);
  assert.ok(raw.windows.every(w => historicalWindow(raw, w, now)));
});

test("uncalibrated pools keep raw 5h while an exhausted week still gates to zero", () => {
  const raw = quota(100, 100); raw.fiveHourWeeklyRatio = null;
  assert.equal(effectiveQuotaWindows(raw)[0].remainingPercent, 100);
  raw.windows[1].remainingPercent = 0;
  assert.equal(effectiveQuotaWindows(raw)[0].remainingPercent, 0);
  raw.fiveHourWeeklyRatio = .4; raw.windows[1].remainingPercent = 10;
  assert.equal(effectiveQuotaWindows(raw)[0].remainingPercent, 25);
});
