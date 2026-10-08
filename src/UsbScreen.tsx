import { lightQuotaColors, quotaBandForPercent } from "./quota-colors";
import { displayProvider, displayWindows, freshQuota, nextProviderReset, resetText } from "./monitor-display";
import { orbDisplay } from "./orb";
import { useState, useEffect, useRef, type CSSProperties } from "react";
import type { Snapshot, UsbThemeDefinition } from "../contracts/hud-api";
import { t } from "./i18n";
import "./usb-screen.css";

export const usbThemes = [
  { id: "usb-mint", name: "薄荷初音" },
  { id: "usb-day", name: "白昼" }, { id: "usb-quad", name: "四平台" },
];
const providers = [["chatgpt", "ChatGPT", "codex.svg"], ["claude", "Claude", "claude.png"], ["antigravity", "Antigravity", "antigravity.svg"], ["grok", "Grok", "grok.svg"]];
function percent(value: number | null | undefined) {
  return value == null || !Number.isFinite(value) ? "—" : value > 0 && value < 1 ? "<1%" : `${Math.floor(value)}%`;
}
function Icon({ provider }: { provider: string }) {
  const item = providers.find(p => p[0] === (provider === "codex" || provider === "codex_usage" ? "chatgpt" : provider));
  return item ? <img src={`/providers/${item[2]}`} alt={item[1]} /> : <span>?</span>;
}
export function UsbScreen({ snapshot, themeId, now, error, customThemes = [] }: { snapshot: Snapshot | null; themeId: string; now: number; error?: string | null; customThemes?: UsbThemeDefinition[] }) {
  const screen = useRef<HTMLDivElement>(null);
  const [bounds, setBounds] = useState({ width: 320, height: 170 });
  useEffect(() => {
    const observer = new ResizeObserver(([entry]) => { if (entry.contentRect.width && entry.contentRect.height) setBounds({ width: entry.contentRect.width, height: entry.contentRect.height }); });
    if (screen.current) observer.observe(screen.current);
    return () => observer.disconnect();
  }, []);
  const [startedAt] = useState(now);
  const elapsed = Math.max(0, now - startedAt);
  const custom = customThemes.find(theme => theme.id === themeId);
  const palette = custom ? Object.fromEntries(Object.entries(custom.palette).map(([key, value]) => [`--${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}`, value])) as CSSProperties : undefined;
  const selectedTheme = usbThemes.some(theme => theme.id === themeId) ? themeId : "usb-mint";
  const columns = custom ? custom.layout === "columns" : selectedTheme === "usb-quad";
  const portrait = bounds.height > bounds.width;
  const scale = Math.min(bounds.width / (portrait ? 160 : 320), bounds.height / (portrait ? 340 : 170));
  const pageSize = 4;
  const pages = Math.max(1, Math.ceil((snapshot?.accounts.length ?? 0) / pageSize));
  const page = Math.floor(elapsed / 8000) % pages;
  return <div className={`usb-screen ${custom ? "usb-custom" : selectedTheme}`} style={{ ...Object.fromEntries(["red", "orange", "blue", "green"].map((color, index) => [`--quota-${color}`, lightQuotaColors[index]])), ...palette } as CSSProperties} ref={screen}><div className={`usb-content ${portrait ? "usb-portrait" : ""}`} style={{ width: bounds.width / scale, height: bounds.height / scale, transform: `scale(${scale})` }}>
    {columns ? <div className="usb-columns">{providers.map(([id, name]) => {
      const total = snapshot?.providerTotals?.[id] ?? (snapshot?.totalQuota?.providerId === id ? snapshot.totalQuota : undefined);
      const display = orbDisplay({ totalQuota: total, recommendation: total?.recommendation ?? { state: "unknown", accountId: null, estimatedAvailableAt: null, reason: "no_data", coverage: { selected: 0, known: 0, unknown: 0, complete: false } } }, now);
      const amount = error ? "—" : display.text;
      const timeLines = error ? null : display.timeLines;
      const nextReset = error ? null : nextProviderReset(snapshot, id, now);
      const accountCount = snapshot?.accounts.filter(account => displayProvider(account.providerId) === id).length;
      return <section className="usb-platform" key={id}>
        <Icon provider={id} /><span className="usb-platform-name">{name}</span>
        <div className="usb-value">{timeLines ? <strong className="usb-countdown">{timeLines.map(line => <span key={line}>{line}</span>)}</strong> : <strong className={`usb-total ${amount.endsWith("%") ? quotaBandForPercent(total?.percent) : ""}`} style={{ fontSize: `${Math.min(28, 112 / amount.length)}px` }}>{amount}</strong>}</div>
        <footer><strong aria-label={t("下次额度重置")}>{resetText(nextReset, now)}</strong><strong className="usb-account-count" aria-label={t("账号数量")}>{accountCount ?? "—"}</strong></footer>
      </section>;
    })}</div> : snapshot?.accounts.length ? <div className="usb-columns usb-account-columns" style={{ gridTemplateColumns: `repeat(${portrait ? Math.min(2, snapshot.accounts.length) : Math.min(4, snapshot.accounts.length)}, minmax(0, 1fr))`, gridTemplateRows: portrait && snapshot.accounts.length > 2 ? "repeat(2, minmax(0, 1fr))" : "1fr" }}>
      {snapshot.accounts.slice(page * pageSize, page * pageSize + pageSize).map(account => {
        const quota = snapshot.quotas.find(q => q.accountId === account.id);
        const limits = displayWindows(quota, account.providerId);
        const limitPages = Math.max(1, Math.ceil(limits.length / 2));
        const limitPage = Math.floor(elapsed / (8000 * pages)) % limitPages;
        const known = !error && freshQuota(quota, now);
        return <section className={`usb-platform usb-account-card ${known ? "" : "usb-uncertain"}`} key={account.id}>
          <Icon provider={account.providerId}/><strong className="usb-account-name" title={account.displayName}>{account.displayName}</strong>
          <div className="usb-account-values">{limits.length ? limits.slice(limitPage * 2, limitPage * 2 + 2).map(limit => <div className="usb-account-limit" key={limit.id}>
            <div title={limit.label}><strong className={known && limit.measurement === "percent" ? quotaBandForPercent(limit.remainingPercent) : ""}>{!known || limit.measurement === "unknown" ? "—" : limit.measurement === "unlimited" ? "∞" : percent(limit.remainingPercent)}</strong></div>
            <span aria-label={t("下次额度重置")}>{known ? resetText(limit.resetsAt, now) : "—"}</span>
          </div>) : <strong className="usb-total">—</strong>}</div>
        </section>;
      })}
    </div> : <div className="usb-empty">{t(snapshot ? "尚未选择账号" : "正在读取配额…")}</div>}
  </div></div>;
}
