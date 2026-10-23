import { useEffect, useState } from "react";
import type { AccountSummary, Settings } from "../contracts/hud-api";
import { t } from "./i18n";

export const quotaProviders = [
  { id: "chatgpt", name: "ChatGPT" }, { id: "antigravity", name: "Antigravity" },
  { id: "grok", name: "Grok" }, { id: "claude", name: "Claude" },
];
export const quotaProvider = (id: string) => ["codex", "codex_usage"].includes(id) ? "chatgpt" : id;
export function availableQuotaProviders(accounts: AccountSummary[]) {
  const ids = [...new Set(accounts.filter(a => a.selected).sort((a, b) => a.order - b.order).map(a => quotaProvider(a.providerId)))];
  return ids.flatMap(id => quotaProviders.filter(p => p.id === id));
}
export function selectedQuotaProvider(settings: Settings, accounts: AccountSummary[]) {
  const available = availableQuotaProviders(accounts);
  return available.find(p => p.id === settings.display.quotaProvider)?.id ?? available[0]?.id ?? "";
}
const profiles: Record<string, [string, string][]> = {
  chatgpt: [["plus", "Plus / Team / Business · 1×"], ["pro5x", "Pro 5×"], ["pro10x", "Pro 10×"], ["pro20x", "Pro 20×"]],
  claude: [["pro", "Pro · 1×"], ["max5x", "Max 5×"], ["max20x", "Max 20×"]],
  antigravity: [["pro", "Google AI Pro · 1×"], ["ultra5x", "Google AI Ultra 5×"]],
  grok: [["supergrok", "SuperGrok · 1×"]],
};

export function QuotaSettings({ settings, accounts, busy, onChange }: {
  settings: Settings; accounts: AccountSummary[]; busy: boolean;
  onChange: (display: Partial<Settings["display"]>) => void;
}) {
  const active = selectedQuotaProvider(settings, accounts);
  const available = availableQuotaProviders(accounts);
  const save = (id: string, value: string) => {
    const quotaProfiles = Object.fromEntries(Object.entries(settings.display.quotaProfiles ?? {}).filter(([key]) => accounts.some(a => a.id === key)));
    if (value) quotaProfiles[id] = value; else delete quotaProfiles[id];
    onChange({ quotaProfiles });
  };
  return <>
    <div className="setting-row"><strong>{t("悬浮窗额度显示")}</strong><select aria-label={t("悬浮窗额度显示")} value={active} disabled={busy || !available.length} onChange={e => onChange({ quotaProvider: e.target.value })}>
      {!available.length && <option value="">{t("请先选择账号")}</option>}
      {available.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
    </select></div>
    <p className="quota-help">{t("只累计所选平台中已勾选的账号与额度池，沿用订阅档位估算。")}</p>
    <details className="quota-profile-settings"><summary>{t("订阅额度换算（估算）")}</summary>
      <p className="quota-help">{t("自动读取订阅；未返回订阅时暂按基础档估算，无法识别的档位不累计。可在此指定档位。")}</p>
      <p className="quota-help">{t("所有额度池的 5h／7d 比例根据实际消耗自动学习，每次刷新验证。样本不足时显示原始额度；少量异常保留可靠估计，持续偏离后重新学习。")}</p>
      <p className="quota-help">{t("历史额度及倒计时以半透明显示，成功取得新数据后恢复。")}</p>
      {accounts.filter(a => quotaProvider(a.providerId) === active).map(a => <ProfileRow key={a.id} account={a} value={settings.display.quotaProfiles?.[a.id] ?? ""} busy={busy} save={value => save(a.id, value)} />)}
    </details>
  </>;
}

function ProfileRow({ account, value, busy, save }: { account: AccountSummary; value: string; busy: boolean; save: (value: string) => void }) {
  const [custom, setCustom] = useState(value.startsWith("custom:"));
  const [multiplier, setMultiplier] = useState("1");
  useEffect(() => {
    setCustom(value.startsWith("custom:"));
    const parts = value.split(":");
    setMultiplier(parts[1] ?? "1");
  }, [value, account.providerId]);
  const valid = Number.isFinite(+multiplier) && +multiplier >= 0.01 && +multiplier <= 1000;
  return <div className="quota-profile-row">
    <label><span translate="no">{account.displayName}</span><select aria-label={`${t("订阅档位")} ${account.displayName}`} value={custom ? "custom" : value} disabled={busy} onChange={e => { setCustom(e.target.value === "custom"); if (e.target.value !== "custom") save(e.target.value); }}>
      <option value="">{t("自动（未返回订阅时按基础档）")}</option>
      {profiles[quotaProvider(account.providerId)]?.map(([id, label]) => <option key={id} value={id}>{label}</option>)}
      <option value="custom">{t("自定义")}</option>
    </select></label>
    {custom && <div className="quota-custom"><label>{t("额度倍率")}<input type="number" min="0.01" max="1000" step="any" value={multiplier} disabled={busy} onChange={e => setMultiplier(e.target.value)} /></label><button className="secondary-button" disabled={busy || !valid} onClick={() => save(`custom:${+multiplier}`)}>{t("保存")}</button></div>}
  </div>;
}
