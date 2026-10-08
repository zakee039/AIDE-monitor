import { orbDisplay } from "./orb";
import { useState, useEffect, useRef, type CSSProperties } from "react";
import type { Snapshot, AccountQuota, UsbThemeDefinition } from "../contracts/hud-api";
import { t } from "./i18n";
import "./usb-screen.css";

export const usbThemes = [
  { id: "usb-mint", name: "薄荷初音" },
  { id: "usb-day", name: "白昼" }, { id: "usb-quad", name: "四平台" },
];
const providers = [["chatgpt", "ChatGPT", "codex.svg"], ["claude", "Claude", "claude.png"], ["antigravity", "Antigravity", "antigravity.svg"], ["grok", "Grok", "grok.svg"]];
const providerId = (id: string) => id === "codex" || id === "codex_usage" ? "chatgpt" : id;
function countdown(date: string | null | undefined, now: number) {
  if (!date || !Number.isFinite(Date.parse(date))) return "—";
  const minutes = Math.ceil((Date.parse(date) - now) / 60000);
  if (minutes <= 0) return t("待核实");
  return minutes < 60 ? `${minutes}m` : minutes < 1440 ? `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}m` : `${Math.floor(minutes / 1440)}d${Math.floor(minutes % 1440 / 60)}h`;
}
function percent(value: number | null | undefined) {
  return value == null || !Number.isFinite(value) ? "—" : value > 0 && value < 1 ? "<1%" : `${Math.floor(value)}%`;
}
function Icon({ provider }: { provider: string }) {
  const item = providers.find(p => p[0] === providerId(provider));
  return item ? <img src={`/providers/${item[2]}`} alt={item[1]} /> : <span>?</span>;
}
function windows(quota: AccountQuota | undefined, provider: string) {
  return quota?.windows.filter(w => w.applicability !== "not_applicable" && (provider === "antigravity" ? w.scope === "feature:gemini" : provider === "grok" || w.scope === "base")).sort((a, b) => (a.durationSeconds ?? 0) - (b.durationSeconds ?? 0)) ?? [];
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
  const portrait = columns && bounds.height > bounds.width;
  const scale = Math.min(bounds.width / (portrait ? 160 : 320), bounds.height / (portrait ? 340 : 170));
  const pageSize = Math.max(1, Math.floor((bounds.height / scale - 74) / 32));
  const pages = Math.max(1, Math.ceil((snapshot?.accounts.length ?? 0) / pageSize));
  const page = Math.floor(elapsed / 8000) % pages;
  const recommendation = snapshot?.recommendation;
  const account = snapshot?.accounts.find(a => a.id === recommendation?.accountId);
  const available = (state?: string, date?: string | null) => error ? "—" : state === "now" ? "now" : state === "waiting" && date && Date.parse(date) > now ? countdown(date, now) : "—";
  return <div className={`usb-screen ${custom ? "usb-custom" : selectedTheme}`} style={palette} ref={screen}><div className={`usb-content ${portrait ? "usb-portrait" : ""}`} style={{ width: bounds.width / scale, height: bounds.height / scale, transform: `scale(${scale})` }}>
    {columns ? <div className="usb-columns">{providers.map(([id, name]) => {
      const total = snapshot?.providerTotals?.[id] ?? (snapshot?.totalQuota?.providerId === id ? snapshot.totalQuota : undefined);
      const display = orbDisplay({ totalQuota: total, recommendation: total?.recommendation ?? { state: "unknown", accountId: null, estimatedAvailableAt: null, reason: "no_data", coverage: { selected: 0, known: 0, unknown: 0, complete: false } } }, now);
      const amount = error ? "—" : display.text;
      const timeLines = error ? null : display.timeLines;
      const count = snapshot?.accounts.filter(a => providerId(a.providerId) === id).length ?? 0;
      return <section className="usb-platform" key={id}>
        <Icon provider={id} /><span className="usb-platform-name">{name}</span>
        <div className="usb-value">{timeLines ? <strong className="usb-countdown">{timeLines.map(line => <span key={line}>{line}</span>)}</strong> : <strong className="usb-total" style={{ fontSize: `${Math.min(28, 112 / amount.length)}px` }}>{amount}</strong>}</div>
        <footer><strong aria-label={`${t("账号")} ${count}`}>{count}</strong></footer>
      </section>;
    })}</div> : <>
      <header className="usb-heading"><span>AIDE <b>MONITOR</b></span><span>{pages > 1 ? `${page + 1}/${pages} · ` : ""}{new Date(now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })}</span></header>
      <div className="usb-account-list">{snapshot?.accounts.length ? snapshot.accounts.slice(page * pageSize, page * pageSize + pageSize).map(a => {
        const quota = snapshot.quotas.find(q => q.accountId === a.id);
        const limits = windows(quota, a.providerId);
        const limitPages = Math.max(1, Math.ceil(limits.length / 2));
        const limitPage = Math.floor(elapsed / (8000 * pages)) % limitPages;
        const uncertain = quota?.freshness !== "fresh" || !!quota?.error;
        return <div className={`usb-account ${uncertain ? "usb-uncertain" : ""}`} key={a.id}>
          <div className="usb-identity"><Icon provider={a.providerId} /><strong>{a.displayName}</strong></div>
          <div className="usb-limits">{limits.length ? limits.slice(limitPage * 2, limitPage * 2 + 2).map(w => <div key={w.id}><small>{w.durationSeconds === 18000 ? "5h" : w.durationSeconds === 604800 ? "7d" : w.label}</small><strong>{w.measurement === "unlimited" ? "∞" : w.measurement === "unknown" ? "—" : percent(w.remainingPercent)}</strong><span>{uncertain ? t("待核实") : countdown(w.resetsAt, now)}</span></div>) : <span>{t("暂无配额")}</span>}</div>
        </div>;
      }) : <div className="usb-empty">{t(snapshot ? "尚未选择账号" : "正在读取配额…")}</div>}</div>
      {!custom && selectedTheme === "usb-mint" ? <div className="onion-divider" aria-hidden="true"><i /><i /><i /></div> : <div className="usb-divider" />}
      <footer className="usb-footer">{!custom && selectedTheme === "usb-mint" && <img src="/miku/avatar.png" alt="" />}<div><div className="usb-available"><span>available:</span><strong>{account?.displayName ?? "—"}</strong><b className={recommendation?.state === "now" && !error ? "usb-now" : ""}>{available(recommendation?.state, recommendation?.estimatedAvailableAt)}</b></div></div></footer>
    </>}
  </div></div>;
}
