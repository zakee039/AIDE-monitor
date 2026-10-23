import type { AccountQuota, QuotaWindow, Snapshot } from "../contracts/hud-api.ts";

/** Express the usable balance as a percentage of its own window, capped by the weekly budget. Raw data stays intact. */
export function effectiveQuotaWindows(quota: AccountQuota | undefined): QuotaWindow[] {
  if (!quota) return [];
  return quota.windows.map(window => {
    if (window.durationSeconds !== 18000 || window.applicability === "not_applicable") return window;
    const week = quota.windows.find(w => w.scope === window.scope && w.durationSeconds === 604800 && w.applicability !== "not_applicable");
    if (!week) return window;
    const ratio = quota.fiveHourWeeklyRatio;
    const hourly = window.measurement === "percent" ? window.remainingPercent : null;
    const weekly = week.measurement === "percent" ? week.remainingPercent : null;
    if (ratio == null && weekly !== 0) return window; // Unknown pool: show raw windows without guessing capacity.
    const value = weekly === 0 ? 0 : hourly != null && weekly != null && ratio != null && Number.isFinite(ratio) && ratio > 0 && ratio <= 1
      ? Math.min(hourly, weekly / ratio) : null;
    return { ...window, remainingPercent: value, measurement: value == null ? "unknown" : "percent", exhausted: value == null ? null : value === 0, retainedAt: window.retainedAt ?? week.retainedAt };
  });
}

export function historicalWindow(quota: AccountQuota | undefined, window: QuotaWindow | undefined, now: number): boolean {
  if (!quota || !window) return false;
  const expired = (date: string | null | undefined) => !!date && Date.parse(date) <= now;
  return !!window.retainedAt || !!quota.error || quota.origin !== "network" || quota.freshness !== "fresh"
    || !["ok", "refreshing"].includes(quota.status) || expired(quota.validUntil) || expired(window.resetsAt);
}

export function historicalProvider(snapshot: Snapshot | null, provider: string | undefined, now: number): boolean {
  const ids = new Set(snapshot?.accounts.filter(a => (["codex", "codex_usage"].includes(a.providerId) ? "chatgpt" : a.providerId) === provider).map(a => a.id));
  return !!snapshot?.quotas.some(q => ids.has(q.accountId) && q.windows.some(w => w.scope === "base" && w.applicability !== "not_applicable" && historicalWindow(q, w, now)));
}
