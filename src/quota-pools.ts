import type { AccountSummary } from "../contracts/hud-api.ts";

export const poolOptions = (provider: string): { id: string; label: string }[] =>
  provider === "grok" ? [{ id: "shared", label: "共享额度" }] : [];
export const accountPoolOptions = (account: AccountSummary) => account.pools ?? poolOptions(account.providerId);
export function selectedPools(account: AccountSummary, choices: Record<string, string[]>): string[] {
  const options = accountPoolOptions(account);
  const chosen = choices[account.id];
  if (!chosen) return options.slice(0, 1).map(p => p.id);
  if (!options.length) return chosen;
  const valid = chosen.filter(id => options.some(p => p.id === id));
  return valid.length || !chosen.length ? valid : options.slice(0, 1).map(p => p.id);
}
export function displayCount(accounts: AccountSummary[], selection: string[], choices: Record<string, string[]>): number {
  return accounts.filter(a => selection.includes(a.id)).reduce((sum, a) => sum + (accountPoolOptions(a).length ? selectedPools(a, choices).length : 1), 0);
}
