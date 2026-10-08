import type { AccountQuota, QuotaWindow, Snapshot } from "../contracts/hud-api.ts";
export const displayProvider = (id: string) => id === "codex" || id === "codex_usage" ? "chatgpt" : id;
export function displayWindows(quota: AccountQuota | undefined, provider: string): QuotaWindow[] {
  return quota?.windows.filter(w => w.applicability !== "not_applicable" && (provider === "antigravity" ? w.scope === "feature:gemini" : provider === "grok" || w.scope === "base")).sort((a,b)=>(a.durationSeconds??0)-(b.durationSeconds??0)) ?? [];
}
export function freshQuota(quota: AccountQuota | undefined, now: number): boolean {
  return !!quota && quota.freshness === "fresh" && quota.origin === "network" && !quota.error && (quota.status === "ok" || quota.status === "refreshing") && (!quota.validUntil || Date.parse(quota.validUntil) > now);
}
/** Failed refreshes retain their last successful measurements, visibly marked as uncertain. */
export function displayableQuota(quota: AccountQuota | undefined, now: number): boolean {
  return freshQuota(quota, now) || !!(quota?.error && quota.lastSuccessAt && quota.windows.length);
}
/** The next reset in the same pool as the platform total, never the polling timer. */
export function nextProviderReset(snapshot: Snapshot | null, provider: string, now: number): string | null {
  const ids = new Set(snapshot?.accounts.filter(a=>displayProvider(a.providerId)===provider).map(a=>a.id));
  const dates = snapshot?.quotas.filter(q=>ids.has(q.accountId)&&displayableQuota(q,now)).flatMap(q=>q.windows.filter(w=>w.applicability === "required" && w.measurement !== "unlimited" && (provider === "antigravity" ? w.scope === "feature:gemini" : w.scope === "base")).map(w=>w.resetsAt)).filter((date):date is string=>!!date && Date.parse(date)>now) ?? [];
  return dates.sort((a,b)=>Date.parse(a)-Date.parse(b))[0] ?? null;
}
export function resetText(date: string | null | undefined, now: number): string {
  if (!date || !Number.isFinite(Date.parse(date)) || Date.parse(date)<=now) return "—";
  const minutes=Math.ceil((Date.parse(date)-now)/60000);
  return minutes<60?`${minutes}m`:minutes<1440?`${Math.floor(minutes/60)}h${String(minutes%60).padStart(2,"0")}m`:`${Math.floor(minutes/1440)}d${Math.floor(minutes%1440/60)}h`;
}
