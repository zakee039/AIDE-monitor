import type { AccountSummary } from "../contracts/hud-api.ts";

export const poolOptions = (provider: string): { id: string; label: string }[] => provider === "antigravity"
  ? [{ id: "gemini3", label: "Gemini 3" }, { id: "claude", label: "Claude" }, { id: "gemini4", label: "Gemini 4" }]
  : provider === "grok" ? [{ id: "shared", label: "共享额度" }] : [];
export function selectedPools(account: AccountSummary, choices: Record<string, string[]>): string[] {
  return choices[account.id] ?? poolOptions(account.providerId).slice(0, 1).map(p => p.id);
}
export function displayCount(accounts: AccountSummary[], selection: string[], choices: Record<string, string[]>): number {
  return accounts.filter(a => selection.includes(a.id)).reduce((sum, a) => sum + (poolOptions(a.providerId).length ? selectedPools(a, choices).length : 1), 0);
}
