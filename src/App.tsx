import { UsbScreen } from "./UsbScreen";
import { QuotaSettings } from "./QuotaSettings";
import { ProxySettings } from "./ProxySettings";
import { UpdatePanel } from "./UpdatePanel";
import { UsbDisplayPanel } from "./UsbDisplayPanel";
import { ThemeManager } from "./ThemeManager";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { orbDisplay } from "./orb";
import { localize, setLanguage, t } from "./i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AccountAvailability, AccountQuota, AccountSummary, Capabilities, Diagnostics, QuotaWindow, Settings, SettingsPatch, Snapshot, ThemeSummary } from "../contracts/hud-api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowDown, ArrowLeft, ArrowUp, Check, ChevronRight, CircleAlert, EyeOff, Database, Info, Layers3, LoaderCircle, Palette, RefreshCw, Settings2, ShieldCheck, SlidersHorizontal, X } from "lucide-react";
import { call, desktop, errorMessage, hud, internal, type SourceOption } from "./api";
import { defaultTheme, midnightTheme, paperTheme, themeStyle, type ThemeDocument } from "./themes";

interface AppState {
  snapshot: Snapshot | null;
  accounts: AccountSummary[];
  settings: Settings | null;
  themes: ThemeSummary[];
  theme: ThemeDocument;
  sourcePath: string | null;
  capabilities: Capabilities | null;
}

const usbView = new URLSearchParams(location.search).get("view") === "usb";
const initialState: AppState = { snapshot: null, accounts: [], settings: null, themes: [], theme: defaultTheme, sourcePath: null, capabilities: null };
const availabilityReasons: Record<string, string> = { no_data: "暂无配额", stale: "数据已过期", awaiting_confirmation: "等待核实", incomplete_quota: "配额不完整", other_limit: "其他限制", unknown_reset: "重置时间未知", query_failed: "查询失败", cache_only: "仅缓存数据", clock_uncertain: "时间需核实", below_threshold: "低于推荐额度", quota_exhausted: "额度已耗尽", available: "当前可用" };

export function remainingText(window: QuotaWindow): string {
  if (window.applicability === "not_applicable") return "不适用";
  if (window.measurement === "unlimited") return "无限";
  if (window.remainingPercent === null || window.measurement === "unknown") return "未知";
  return window.remainingPercent > 0 && window.remainingPercent < 1 ? "<1%" : `${Math.floor(window.remainingPercent)}%`;
}

export function durationText(date: string | null, now: number): string {
  if (!date || !Number.isFinite(Date.parse(date))) return "时间未知";
  const seconds = Math.ceil((Date.parse(date) - now) / 1000);
  if (seconds <= 0) return "待核实";
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h${String(minutes % 60).padStart(2, "0")}m`;
  return `${Math.floor(hours / 24)}d${String(hours % 24).padStart(2, "0")}h`;
}

function statusText(quota: AccountQuota | undefined, availability: AccountAvailability | undefined): string {
  if (quota?.status === "auth_expired") return "认证过期";
  if (quota?.error) return "查询失败";
  if (quota?.freshness === "stale") return quota.status === "refreshing" ? "过期 · 更新中" : "已过期";
  if (quota?.status === "refreshing") return "更新中";
  if (availability?.state === "now") return "可用";
  if (availability?.state === "waiting") return "等待重置";
  return availabilityReasons[availability?.reason ?? "no_data"] ?? "待核实";
}

function useHudState() {
  const [state, setState] = useState<AppState>(initialState);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mounted = useRef(true);
  const running = useRef(false);
  const dirty = useRef(false);
  const observed = useRef({ instanceId: "", revision: -1 });
  const current = useRef({ instanceId: "", revision: -1 });
  const reload = useCallback(async () => {
    dirty.current = true;
    if (running.current) return;
    running.current = true;
    try {
      while (dirty.current && mounted.current) {
        dirty.current = false;
        const [snapshot, accounts, settings, themes, capabilities, sourcePath, theme] = await Promise.all([
          call("quota.snapshot.get", {}), call("accounts.list", {}), call("settings.get", {}), call("themes.list", {}), call("capabilities.get", {}), !desktop || new URLSearchParams(location.search).get("view") === "settings" ? internal("source_get", {}) : Promise.resolve({ path: null }), usbView ? Promise.resolve(defaultTheme) : internal("theme_get", {}),
        ]);
        if (!mounted.current) break;
        const newestEvent = observed.current;
        if (newestEvent.instanceId && snapshot.instanceId === newestEvent.instanceId && snapshot.revision < newestEvent.revision) { dirty.current = true; continue; }
        if (snapshot.instanceId === current.current.instanceId && snapshot.revision < current.current.revision) continue;
        if (newestEvent.instanceId && snapshot.instanceId !== newestEvent.instanceId) { dirty.current = true; continue; }
        current.current = { instanceId: snapshot.instanceId, revision: snapshot.revision };
        setState({ snapshot: snapshot.data, accounts: accounts.data, settings: settings.data, themes: themes.data, capabilities: capabilities.data, sourcePath: sourcePath.path, theme });
        setError(null);
      }
    } catch (failure) {
      if (mounted.current) setError(errorMessage(failure));
    } finally {
      running.current = false;
      if (mounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let stop: (() => void) | undefined;
    void hud.subscribe(event => {
      if (!mounted.current) return;
      if (observed.current.instanceId === event.instanceId && event.revision < observed.current.revision) return;
      observed.current = { instanceId: event.instanceId, revision: event.revision };
      void reload();
    }).then(unsubscribe => {
      if (!mounted.current) { unsubscribe(); return; }
      stop = unsubscribe;
      // Subscription is registered before the initial authoritative read.
      void reload();
    }).catch(failure => { if (mounted.current) { setError(errorMessage(failure)); setLoading(false); } });
    const poll = window.setInterval(() => { void reload(); }, 20000);
    return () => { mounted.current = false; stop?.(); window.clearInterval(poll); };
  }, [reload]);
  return { state, reload, error, loading };
}

export default function App() {
  const { state, reload, error: readError, loading } = useHudState();
  setLanguage(state.settings?.display.locale === "zh-CN" ? "zh-CN" : "en");
  useEffect(() => { document.documentElement.lang = state.settings?.display.locale === "zh-CN" ? "zh-CN" : "en"; }, [state.settings?.display.locale]);
  const [settingsView, setSettingsView] = useState(new URLSearchParams(location.search).get("view") === "settings");
  const [now, setNow] = useState(Date.now());
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [collapsed, commitCollapsed] = useState(false);
  const [transitioning, setTransitioning] = useState(false);
  const changing = useRef(false);
  const collapsedValue = useRef(false);
  const setCollapsed = (value: boolean | ((previous: boolean) => boolean)) => {
    if (changing.current) return;
    const next = typeof value === "function" ? value(collapsedValue.current) : value;
    if (next === collapsedValue.current) return;
    changing.current = true;
    setTransitioning(true);
    // Hide content before React swaps shapes; reveal only after native geometry settles.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      collapsedValue.current = next;
      commitCollapsed(next);
      if (!desktop) { setTransitioning(false); changing.current = false; }
    }));
  };
  useEffect(() => {
    if (!desktop || settingsView || usbView) return;
    let stop: (() => void) | undefined;
    let active = true;
    void listen("aide://toggle-collapse", () => setCollapsed(value => !value)).then(unlisten => { if (active) stop = unlisten; else unlisten(); });
    const context = (event: MouseEvent) => { event.preventDefault(); void invoke("aide_hud_context_menu").catch(() => setActionError(t("暂时无法完成操作，请重试。"))); };
    const doubleClick = (event: MouseEvent) => { event.preventDefault(); event.stopImmediatePropagation(); };
    window.addEventListener("contextmenu", context, true);
    window.addEventListener("dblclick", doubleClick, true);
    return () => { active = false; stop?.(); window.removeEventListener("contextmenu", context, true); window.removeEventListener("dblclick", doubleClick, true); };
  }, [settingsView]);
  useEffect(()=>{if(!desktop||settingsView||usbView)return;let active=true;let stop:(()=>void)|undefined;void listen<boolean>('aide://set-collapse',event=>setCollapsed(event.payload)).then(fn=>{if(active)stop=fn;else fn();});return()=>{active=false;stop?.();};},[settingsView]);
  useEffect(()=>{if(desktop&&!settingsView&&!usbView&&!collapsed)void invoke('aide_theme_resume').catch(()=>{});},[collapsed,settingsView]);
  const panelRef = useRef<HTMLDivElement>(null);
  const layoutQueue = useRef<Promise<unknown>>(Promise.resolve());
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer); }, []);
  useEffect(() => {
    if (!desktop || settingsView || usbView) return;
    const panel = panelRef.current;
    if (!panel) return;
    let timer: number | undefined;
    let previousSize = "";
    let active = true;
    let pending: { width: number; height: number } | undefined;
    let resizing = false;
    const resize = async () => {
      if (resizing) return;
      resizing = true;
      try {
        while (pending && active) {
          const size = pending;
          pending = undefined;
          const scheduled = layoutQueue.current.catch(() => {}).then(() => {
            if (active) return internal("window_layout", { ...size, collapsed });
          });
          layoutQueue.current = scheduled;
          await scheduled;
          if (active) { setTransitioning(false); changing.current = false; }
        }
      } catch (failure) { if (active) setActionError(errorMessage(failure)); }
      finally { resizing = false; if (active) { setTransitioning(false); changing.current = false; } }
    };
    const measure = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const bounds = panel.getBoundingClientRect();
        // A fixed orb must not grow by one logical pixel when DPI introduces
        // fractional measurement noise. Expanded content still rounds upward.
        const size = collapsed ? { width: 43, height: 43 } : { width: Math.max(40, Math.min(1000, Math.ceil(bounds.width))), height: Math.max(40, Math.min(560, Math.ceil(bounds.height))) };
        const key = `${size.width}:${size.height}`;
        if (key !== previousSize) {
          previousSize = key;
          pending = size;
          void resize();
        }
      }, 80);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(panel);
    measure();
    return () => { active = false; observer.disconnect(); window.clearTimeout(timer); };
  }, [settingsView, collapsed]);

  const perform = async (key: string, action: () => Promise<unknown>, success?: string) => {
    if (busy) return;
    setBusy(key); setActionError(null); setNotice(null);
    try { await action(); await reload(); if (success) setNotice(success); }
    catch (failure) { setActionError(errorMessage(failure)); }
    finally { setBusy(null); }
  };

  const openSettings = () => {
    if (desktop) void perform("settings", () => call("window.control", { action: "open_settings" }));
    else { setSettingsView(true); history.replaceState(null, "", "?view=settings"); }
  };
  return localize(<main onMouseDownCapture={event => {
    if (!settingsView && (state.settings?.display.positionLocked || usbView) && (event.target as HTMLElement).closest('[data-tauri-drag-region],.quota-orb')) event.stopPropagation();
  }} className={`app-stage ${desktop ? "desktop" : "browser"} ${settingsView ? "settings-stage" : "hud-stage"} ${usbView ? "usb-stage" : ""} ${state.settings?.display.positionLocked || usbView ? "position-locked" : ""}`}>
    {settingsView ?
      <SettingsView state={state} loading={loading} error={actionError ?? readError} notice={notice} busy={busy} perform={perform} reload={reload} onBack={() => { setSettingsView(false); history.replaceState(null, "", location.pathname); }} /> :
      <div ref={panelRef} className={`hud-shell ${collapsed ? "is-collapsed" : ""} ${transitioning ? "is-transitioning" : ""} ${usbView ? "" : `theme-${state.theme.id}`}`} style={usbView ? undefined : themeStyle(state.theme)}>
        {usbView ? <UsbScreen snapshot={state.snapshot} customThemes={state.settings?.usbDisplay.customThemes} themeId={state.settings?.usbDisplay.themeId ?? "usb-mint"} now={now} error={readError} /> : collapsed && state.snapshot ? <QuotaOrb locked={state.settings?.display.positionLocked ?? false} snapshot={state.snapshot} now={now} onExpand={() => setCollapsed(false)} /> : loading && !state.snapshot ? <div className="hud-empty" data-tauri-drag-region>正在读取配额…</div> : state.snapshot ?
          <HudContent snapshot={state.snapshot} theme={state.theme} now={now} onSettings={usbView ? undefined : openSettings} onRefresh={usbView ? undefined : () => { void perform("refresh-all", () => call("refresh.request", {})); }} onHide={usbView ? undefined : () => { void perform("hide", () => call("window.control", { action: "hide" })); }} onCollapse={usbView ? undefined : () => setCollapsed(true)} busy={busy} /> :
          <div className="hud-empty"><button className="text-button" onClick={() => { void reload(); }}>读取失败 · 点击重试</button></div>}
        {!usbView && !collapsed && (actionError ?? readError) && <div className="hud-error" role="alert">{actionError ?? readError}</div>}
      </div>}
    {!desktop && !settingsView && !usbView && <div className="browser-caption"><span>虚构数据预览 · 更多操作位于托盘右键菜单</span><button className="text-button" onClick={openSettings}>预览设置</button></div>}
  </main>);
}

function DemoBanner() { return localize(<div className="demo-banner"><span className="demo-label">DEMO</span><span>浏览器预览 · 全部账号与配额均为虚构</span></div>); }

// Absent limits are omitted only when the adapter identifies them as inapplicable.
// Unknown/malformed limits retain a placeholder instead of suggesting extra quota.
function displayWindows(quota?: AccountQuota): (QuotaWindow | undefined)[] {
  if (!quota) return [undefined];
  const windows = quota.windows.filter(window => window.scope === "base" && window.applicability !== "not_applicable");
  return windows.length ? windows.sort((a, b) => (a.durationSeconds ?? 0) - (b.durationSeconds ?? 0)) : [undefined];
}

function windowColumnKey(window?: QuotaWindow): string {
  return window?.durationSeconds != null ? `duration-${window.durationSeconds}` : `unknown-${window?.kind ?? "quota"}`;
}

function HudContent({ snapshot, theme, now, onSettings, onRefresh, onHide, onCollapse, busy }: { snapshot: Snapshot; theme: ThemeDocument; now: number; onSettings?: () => void; onRefresh?: () => void; onHide?: () => void; onCollapse?: () => void; busy?: string | null }) {
  const hasCredits = snapshot.accounts.some(account => account.providerId === "antigravity") || snapshot.quotas.some(quota => quota.resetCreditsAvailable != null);
  const windowColumns = [...new Set(snapshot.accounts.filter(account => account.providerId !== "grok").flatMap(account => (account.providerId === "antigravity" ? ["duration-18000", "duration-604800"] : displayWindows(snapshot.quotas.find(quota => quota.accountId === account.id)).map(windowColumnKey))))].sort((a, b) => {
    const duration = (key: string) => key.startsWith("duration-") ? Number(key.slice(9)) : Infinity;
    return duration(a) - duration(b) || a.localeCompare(b);
  });
  if (!windowColumns.length) windowColumns.push("models");
  // Each limit shares percentage, separator and countdown tracks across rows.
  // Four character tracks separate logical columns without fixed data widths.
  const columns = `max-content minmax(0, max-content)${windowColumns.length ? ` repeat(${windowColumns.length}, 4ch max-content max-content max-content)` : ""}${hasCredits ? " 4ch max-content" : ""}`;
  return localize(<div className="hud-body" data-tauri-drag-region>
    {snapshot.accounts.length ? <>
      <section className="compact-accounts" style={{ gridTemplateColumns: columns }} aria-label="账号配额" data-tauri-drag-region>{snapshot.accounts.map(account => <AccountRow key={account.id} account={account} quota={snapshot.quotas.find(quota => quota.accountId === account.id)} availability={snapshot.availability.find(item => item.accountId === account.id)} now={now} showCredits={hasCredits} windowColumns={windowColumns} interactive={!!onSettings} />)}</section>
    </> : <div className="hud-empty" data-tauri-drag-region><span>{snapshot.source.state === "ready" ? "尚未选择账号" : "尚未连接账号来源"}</span><span className="hud-empty-hint">右键托盘 → 设置</span></div>}
    {theme.id === "default" && <div className="onion-divider" aria-hidden="true"><i /><i /><i /></div>}
    <RecommendationStrip snapshot={snapshot} now={now} onSettings={onSettings} onRefresh={onRefresh} onHide={onHide} onCollapse={onCollapse} busy={busy} />
    <span className="sr-only">{theme.name}</span>
  </div>);
}

function AccountRow({ account, quota, availability, now, showCredits, windowColumns, interactive }: { account: AccountSummary; quota?: AccountQuota; availability?: AccountAvailability; now: number; showCredits: boolean; windowColumns: string[]; interactive: boolean }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setError(null); }, [quota?.lastSuccessAt]);
  const refreshing = pending || quota?.status === "refreshing";
  const status = error ?? statusText(quota, availability);
  const state = error || quota?.error ? "error" : quota?.freshness === "stale" ? "stale" : availability?.state ?? "unknown";
  const antigravity = account.providerId === "antigravity";
  const windows = antigravity ? [quota?.windows.find(window => window.id === "gemini-5h"), quota?.windows.find(window => window.id === "gemini-weekly")] : displayWindows(quota);
  const modelProvider = account.providerId === "grok";
  const refresh = async () => {
    setPending(true); setError(null);
    try { await call("refresh.request", { accountIds: [account.id] }); }
    catch (failure) { setError(errorMessage(failure)); }
    finally { setPending(false); }
  };
  return localize(<div className={`compact-row state-${state}`} data-tauri-drag-region>
    <button className={`row-refresh ${refreshing ? "refreshing" : ""}`} aria-label={`刷新 ${account.displayName}`} disabled={!interactive || refreshing} onClick={() => { void refresh(); }}><RefreshCw size={13} className={refreshing ? "spin" : ""} /></button>
    <span className="compact-name" translate="no" data-tauri-drag-region><ProviderIcon provider={account.providerId} />{account.displayName}</span>
    {modelProvider ? <div className="model-quotas">{quota?.windows.map(w => <span className={`model-quota ${quotaBand(w)}`} key={w.id} title={w.label}><small>{w.label}</small><strong>{remainingText(w)}</strong><span>· {durationText(w.resetsAt, now)}</span></span>) ?? <span>—</span>}</div> : windows.map((window, index) => <span className={`compact-quota ${quotaBand(window)}`} style={{ gridColumn: `${4 + (antigravity ? windowColumns.indexOf(index === 0 ? "duration-18000" : "duration-604800") : windowColumns.indexOf(windowColumnKey(window))) * 4} / span 3` }} key={window?.id ?? index} aria-label={`${window?.label ?? "?"} 剩余 ${window ? remainingText(window) : "未知"}，${window ? durationText(window.resetsAt, now) : "待查询"}`} data-tauri-drag-region><strong>{window ? remainingText(window) : "—"}</strong><span className="quota-dot">·</span><span>{window ? durationText(window.resetsAt, now) : "—"}</span></span>)}
    {showCredits && <span className="reset-credits" style={{ gridColumn: windowColumns.length * 4 + 4 }} aria-label={antigravity ? "gemini额度" : quota?.resetCreditsAvailable != null ? `可用重置次数 ${quota.resetCreditsAvailable}` : "重置次数未知"} data-tauri-drag-region>{antigravity ? "gemini额度" : quota?.resetCreditsAvailable != null ? `R: ${quota.resetCreditsAvailable}` : ""}</span>}
    {state === "error" && <span className="row-state state-error" role="img" aria-label={status}>!</span>}
  </div>);
}

function quotaBand(window?: QuotaWindow): string {
  if (window?.measurement !== "percent" || window.remainingPercent === null) return "";
  const percent = window.remainingPercent;
  return percent < 20 ? "quota-red" : percent < 50 ? "quota-orange" : percent < 80 ? "quota-blue" : "quota-green";
}

function RecommendationStrip({ snapshot, now, onSettings, onRefresh, onHide, onCollapse, busy }: { snapshot: Snapshot; now: number; onSettings?: () => void; onRefresh?: () => void; onHide?: () => void; onCollapse?: () => void; busy?: string | null }) {
  const recommendation = snapshot.recommendation;
  const account = snapshot.accounts.find(item => item.id === recommendation.accountId);
  const refreshing = busy === "refresh-all" || snapshot.quotas.some(quota => quota.status === "refreshing");
  return localize(<div className={`compact-recommendation state-${recommendation.state}`} data-tauri-drag-region>
    <span className="availability-copy" data-tauri-drag-region><span>available:</span><strong translate="no" data-tauri-drag-region>{account?.displayName ?? "—"}</strong><span>·</span><span className="usable-time">{recommendation.state === "now" ? "now" : recommendation.state === "waiting" ? durationText(recommendation.estimatedAvailableAt, now) : "—"}</span></span>
    <div className="hud-actions">
      <button className="row-refresh" aria-label="设置" disabled={!onSettings || !!busy} onClick={onSettings}><Settings2 size={13} /></button>
      <button className="row-refresh" aria-label="刷新全部" disabled={!onRefresh || !!busy || refreshing || !snapshot.accounts.length} onClick={onRefresh}><RefreshCw size={13} className={refreshing ? "spin" : ""} /></button>
      <button className="row-refresh" aria-label="隐藏悬浮窗" disabled={!onHide || !!busy} onClick={onHide}><EyeOff size={13} /></button>
      {onCollapse && <button className="row-refresh hud-collapse" aria-label="折叠悬浮窗" onClick={onCollapse}><ChevronRight size={14} /></button>}
    </div>
  </div>);
}

function QuotaOrb({ snapshot, now, onExpand, locked }: { locked: boolean; snapshot: Snapshot; now: number; onExpand: () => void }) {
  const total = snapshot.totalQuota;
  const { waiting, timeLines, text } = orbDisplay(snapshot, now);
  const drag = useRef({ x: 0, y: 0, moved: false, pressed: false });
  return <button title={`${total?.providerId ?? "ChatGPT"}${total?.estimated ? ` · ${t("按基础档估算")}` : ""}`} className={`quota-orb ${waiting ? "orb-waiting" : ""} ${!timeLines && text.length > 4 ? "orb-long" : ""} ${!timeLines && text.length > 5 ? "orb-extra-long" : ""}`} aria-label={`${t("展开悬浮窗")} · ${t(waiting ? "等待重置" : "估算总额度")} ${text}${total?.partial ? ` · ${t("部分账号数据不可用")}` : ""}`} onPointerDown={event => {
    if (event.button === 0) drag.current = { x: event.clientX, y: event.clientY, moved: false, pressed: true };
  }} onPointerMove={event => {
    const origin = drag.current;
    if (locked || !desktop || !origin.pressed || origin.moved || !(event.buttons & 1) || Math.hypot(event.clientX - origin.x, event.clientY - origin.y) < 4) return;
    origin.moved = true;
    void getCurrentWindow().startDragging().catch(() => {});
  }} onPointerUp={() => { drag.current.pressed = false; }} onPointerCancel={() => { drag.current.pressed = false; }} onClick={event => {
    if (event.detail === 0 || !drag.current.moved) onExpand();
    drag.current.moved = false;
  }}><span className={timeLines ? "orb-countdown" : undefined}>{timeLines ? timeLines.map(line => <span key={line}>{line}</span>) : text}</span></button>;
}

type Performer = (key: string, action: () => Promise<unknown>, success?: string) => Promise<void>;
type SettingSection = "general" | "display" | "accounts" | "themes" | "usb" | "about";

function SettingsView({ state, loading, error, notice, busy, perform, reload, onBack }: { state: AppState; loading: boolean; error: string | null; notice: string | null; busy: string | null; perform: Performer; reload: () => Promise<void>; onBack: () => void }) {
  const [section, setSection] = useState<SettingSection>("general");
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [sources, setSources] = useState<SourceOption[]>([]);
  const [startup, setStartup] = useState<boolean | null>(null);
  useEffect(() => { void internal("startup", {}).then(v => setStartup(v.enabled)).catch(() => setStartup(null)); }, []);
  const openSources = () => { void perform("sources", async () => { setSources(await internal("sources_get", {})); setSourcesOpen(true); }); };
  const pickSource = (id: string) => { void perform("source-pick", async () => { const { path } = await internal("sources_pick", {}); if (path) setSources(current => current.map(s => s.id === id ? { ...s, path, enabled: true } : s)); }); };

  const [selection, setSelection] = useState<string[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [interval, setIntervalValue] = useState("300");
  const savedSelection = JSON.stringify(state.accounts.filter(account => account.selected).sort((a, b) => a.order - b.order).map(account => account.id));
  useEffect(() => { setSelection(JSON.parse(savedSelection) as string[]); }, [savedSelection]);
  useEffect(() => { if (state.settings) setIntervalValue(String(state.settings.refreshIntervalSeconds)); }, [state.settings?.refreshIntervalSeconds]);
  const settings = state.settings;
  const patch = (key: string, fields: Omit<SettingsPatch, "expectedRevision">) => { if (settings) void perform(key, () => call("settings.update", { expectedRevision: settings.settingsRevision, ...fields })); };
  const chooseSource = openSources;
  const rescan = () => { void perform("rescan", () => internal("source_rescan", {}), "账号列表已重新读取。"); };
  const changeOrder = (id: string, direction: number) => setSelection(current => { const index = current.indexOf(id); const destination = index + direction; if (index < 0 || destination < 0 || destination >= current.length) return current; const next = [...current]; [next[index], next[destination]] = [next[destination], next[index]]; return next; });
  const selectionDirty = JSON.stringify(selection) !== JSON.stringify(state.accounts.filter(account => account.selected).sort((a, b) => a.order - b.order).map(account => account.id));
  const nav = [{ id: "general", label: "常规", icon: <SlidersHorizontal size={17} /> }, { id: "display", label: "窗口与显示", icon: <Settings2 size={17} /> }, { id: "accounts", label: "账号", icon: <Database size={17} /> }, { id: "themes", label: "外观", icon: <Palette size={17} /> }, { id: "usb", label: "独立监视屏", icon: <Layers3 size={17} /> }, { id: "about", label: "关于", icon: <Info size={17} /> }] as const;

  return localize(<div className="settings-shell" style={themeStyle(defaultTheme)}>
    {sourcesOpen && <SourceDialog error={error} sources={sources} setSources={setSources} busy={busy} onClose={() => setSourcesOpen(false)} onPick={pickSource} onSave={() => { void perform("sources-save", async () => { await internal("sources_save", { sources }); setSourcesOpen(false); }, "数据源已保存并扫描。"); }} />}
    <header className="settings-header" data-tauri-drag-region><div className="brand" data-tauri-drag-region><img className="brand-mark" src="/aide-mark.svg" alt="" /><span className="brand-name" data-tauri-drag-region>AIDE monitor <span className="brand-subtle">/ 设置</span></span></div><div className="header-actions">{!desktop && <button className="text-button return-button" onClick={onBack}><ArrowLeft size={14} />返回 HUD</button>}</div></header>
    {!desktop && <DemoBanner />}
    <div className="settings-layout"><aside className="settings-sidebar"><div className="sidebar-kicker">WORKSPACE</div><nav aria-label="设置分区">{nav.map(item => <button key={item.id} className={`nav-button ${section === item.id ? "selected" : ""}`} onClick={() => setSection(item.id)}>{item.icon}<span>{item.label}</span><ChevronRight size={13} /></button>)}</nav><div className="sidebar-footer"><ShieldCheck size={15} /><span>凭据只留在本机</span><small>v{state.capabilities?.appVersion ?? "—"}</small></div></aside>
    <section className="settings-content">
      {(error || notice) && <div className={error ? "settings-alert error" : "settings-alert success"} role={error ? "alert" : "status"}>{error ? <CircleAlert size={16} /> : <Check size={16} />}<span>{error ?? notice}</span></div>}
      {loading && !settings ? <div className="empty-state"><LoaderCircle size={24} className="spin" /><span>正在读取设置</span></div> : <>
      {section === "general" && <>
        <SettingsCard title="语言" icon={<Settings2 size={17} />}><div className="setting-row"><div><strong>界面语言</strong></div><select aria-label="界面语言" value={settings?.display.locale === "zh-CN" ? "zh-CN" : "en"} disabled={busy !== null || !settings} onChange={event => patch("language", { display: { locale: event.target.value as "en" | "zh-CN" } })}><option value="en">English</option><option value="zh-CN">简体中文</option></select></div></SettingsCard>
        <SettingsCard title="数据源" icon={<Database size={17} />} action={<button className="secondary-button" disabled={busy !== null} onClick={openSources}>选择源</button>}>{state.snapshot?.source.error && <p className="source-error" role="alert">{t("数据源暂不可用")} ({state.snapshot.source.error.code})</p>}</SettingsCard>
        <SettingsCard title="配额刷新" icon={<RefreshCw size={17} />}><ToggleRow title="自动刷新" detail="定期获取所选账号的最新配额" checked={settings?.autoRefresh ?? false} disabled={busy !== null || !settings} onChange={checked => patch("auto-refresh", { autoRefresh: checked })} /><div className="setting-row"><div><strong>刷新间隔</strong></div><select aria-label="刷新间隔" value={interval} disabled={busy !== null || !settings} onChange={event => { setIntervalValue(event.target.value); patch("interval", { refreshIntervalSeconds: Number(event.target.value) }); }}><option value="60">1 分钟</option><option value="300">5 分钟</option><option value="600">10 分钟</option><option value="900">15 分钟</option><option value="1800">30 分钟</option>{![60, 300, 600, 900, 1800].includes(settings?.refreshIntervalSeconds ?? 300) && <option value={settings?.refreshIntervalSeconds}>{settings?.refreshIntervalSeconds} 秒</option>}</select></div></SettingsCard>
        {settings && <ProxySettings settings={settings} reload={reload} />}

      </>}
      {section === "display" && <>
        <SectionHeading eyebrow="WINDOW & DISPLAY" title="窗口与显示" description="" />
        <SettingsCard title="窗口与显示" icon={<Settings2 size={17} />}><ToggleRow title="开机自启" detail="" checked={startup ?? false} disabled={busy !== null || startup === null} onChange={enabled => { void perform("startup", async () => { setStartup((await internal("startup", { enabled })).enabled); }); }} /><ToggleRow title="始终置顶" detail="让 HUD 保持在其他窗口上方" checked={settings?.display.alwaysOnTop ?? true} disabled={busy !== null || !settings} onChange={checked => patch("pin", { display: { alwaysOnTop: checked } })} /><ToggleRow title="锁定位置" detail="" checked={settings?.display.positionLocked ?? false} disabled={busy !== null || !settings} onChange={checked => patch("position-lock", { display: { positionLocked: checked } })} /><ToggleRow title="隐私模式" detail="用“账号 1”等名称遮罩账号显示" checked={settings?.display.privacyMode ?? false} disabled={busy !== null || !settings} onChange={checked => patch("privacy", { display: { privacyMode: checked } })} /></SettingsCard>
        <SettingsCard title="悬浮窗额度" icon={<Settings2 size={17} />}>{settings && <QuotaSettings settings={settings} accounts={state.accounts} busy={busy !== null} onChange={display => patch("quota-display", { display })} />}</SettingsCard>
      </>}
      {section === "accounts" && <><SectionHeading eyebrow="YOUR ACCOUNTS" title="账号" description="勾选要显示的账号，并调整 HUD 中的顺序。" />
        <div className="account-selection-header"><span>{selection.length} 个已选择 / {state.accounts.length} 个账号</span><button className="text-button" disabled={busy !== null} onClick={rescan}><RefreshCw size={14} className={busy === "rescan" ? "spin" : ""} />重新扫描</button></div>
        <div className="selection-columns"><span>账号</span><span>代理</span><span>自动刷新</span><span>显示名称</span><span>排序</span></div><div className="selection-list">{state.accounts.length ? [...state.accounts].sort((a, b) => { const ai = selection.indexOf(a.id); const bi = selection.indexOf(b.id); return (ai < 0 ? 1000 + a.order : ai) - (bi < 0 ? 1000 + b.order : bi); }).map(account => <div className={`selection-row ${selection.includes(account.id) ? "chosen" : ""}`} key={account.id}><label><input type="checkbox" checked={selection.includes(account.id)} disabled={busy !== null || account.support === "unsupported"} onChange={event => setSelection(current => event.target.checked ? [...current, account.id] : current.filter(id => id !== account.id))} /><span className="selection-check"><Check size={12} /></span><ProviderIcon provider={account.providerId} /><span className="selection-identity"><strong translate="no">{account.displayName}</strong></span></label><select aria-label={`代理 ${account.displayName}`} value={settings?.accountProxies?.[account.id] ?? ""} disabled={busy !== null || !settings} onChange={event => {
          const overrides = { ...settings?.accountProxies };
          if (event.target.value) overrides[account.id] = event.target.value; else delete overrides[account.id];
          patch("account-proxy", { accountProxies: overrides });
        }}><option value="">系统</option>{settings?.proxies.map((proxy, index) => <option key={proxy.id} value={proxy.id}>{proxy.name || `${t("代理")} ${index + 1}`}</option>)}</select><select aria-label={`自动刷新 ${account.displayName}`} value={settings?.accountRefresh?.[account.id] ?? ""} disabled={busy !== null || !settings} onChange={event => {
          const overrides = { ...settings?.accountRefresh };
          if (event.target.value === "") delete overrides[account.id]; else overrides[account.id] = Number(event.target.value);
          patch("account-refresh", { accountRefresh: overrides });
        }}><option value="">-</option><option value="0">禁止</option><option value="60">1min</option><option value="300">5min</option><option value="900">15min</option><option value="3600">1h</option></select><AliasInput account={account} reload={reload} />{<div className={`order-actions ${selection.includes(account.id) ? "" : "order-placeholder"}`}><button className="icon-button" aria-label={`上移 ${account.displayName}`} disabled={busy !== null || selection.indexOf(account.id) === 0} onClick={() => changeOrder(account.id, -1)}><ArrowUp size={14} /></button><button className="icon-button" aria-label={`下移 ${account.displayName}`} disabled={busy !== null || selection.indexOf(account.id) === selection.length - 1} onClick={() => changeOrder(account.id, 1)}><ArrowDown size={14} /></button></div>}</div>) : <div className="empty-state"><Database size={24} /><strong>尚未发现账号</strong><span>选择含 auth.json 或 codex_accounts.json 的目录，再重新扫描。</span><button className="secondary-button" disabled={busy !== null} onClick={chooseSource}>选择数据目录</button></div>}</div>
        <div className="form-actions"><span>最多显示 {state.capabilities?.maxRefreshAccounts ?? 100} 个账号</span><button className="primary-button" disabled={busy !== null || !settings || !selectionDirty || selection.length > (state.capabilities?.maxRefreshAccounts ?? 100)} onClick={() => { if (settings) void perform("selection", () => call("accounts.selection.update", { expectedRevision: settings.settingsRevision, accountIds: selection }), "账号选择已保存。"); }}>{busy === "selection" ? <LoaderCircle size={14} className="spin" /> : <Check size={14} />}保存选择</button></div>
      </>}
      {section === "themes" && <ThemeManager onChanged={reload} renderPreview={id => {
        const theme = id === "midnight" ? midnightTheme : id === "paper" ? paperTheme : defaultTheme;
        return state.snapshot ? <div className="desktop-theme-preview"><div className={`hud-shell theme-${id}`} style={themeStyle(theme)}><HudContent snapshot={state.snapshot} theme={theme} now={Date.now()} /></div><div style={themeStyle(theme)}><QuotaOrb locked snapshot={state.snapshot} now={Date.now()} onExpand={()=>{}} /></div></div> : <div className="usb-empty">{t("正在读取配额…")}</div>;
      }} />}
      {section === "usb" && settings && <UsbDisplayPanel settings={settings} snapshot={state.snapshot} busy={busy !== null} onChange={usbDisplay => patch("usb", { usbDisplay })} />}
      {section === "about" && <><SectionHeading eyebrow="SMALL FOOTPRINT" title="专业的 AI IDE 订阅额度监视器" description="Tauri 2 + Rust · 开放接口 · 可自行扩展主题" /><SettingsCard title="AIDE monitor" description={state.capabilities?.appVersion ?? "—"} icon={<Layers3 size={18} />}><p className="about-description">AIDE monitor（AI IDE monitor），专业的订阅额度监视器。集中查看多个 AI IDE 账号的剩余额度、重置时间与最早恢复时间，支持独立代理、桌面悬浮窗与 独立监视屏。</p><div className="about-status"><span className="status-dot" />{desktop ? "正在桌面应用中运行" : "浏览器演示，全部数据均为虚构"}</div></SettingsCard><UpdatePanel /><SettingsCard title="开放能力" description="以当前应用实际提供的方法为准" icon={<SlidersHorizontal size={17} />} action={<button className="text-button" onClick={() => { if (desktop) void perform("docs", () => internal("docs_open", {})); else window.open("/api.html", "_blank", "noopener,noreferrer"); }}>接口说明 <ChevronRight size={14} /></button>}><div className="capability-summary"><div><strong>{state.capabilities?.enabledMethods.length ?? 0}</strong><span>可用方法</span></div><div><strong>{state.capabilities?.apiVersion ?? "—"}</strong><span>接口版本</span></div><div><strong>{state.capabilities?.themeSchemaVersions.join(", ") ?? "—"}</strong><span>主题版本</span></div></div><div className="card-bottom"><span>第一版通过应用内部接口调用。</span><button className="text-button" disabled={busy !== null || !state.capabilities?.enabledMethods.includes("diagnostics.get")} onClick={() => { void perform("diagnostics", async () => { const response = await call("diagnostics.get", {}); setDiagnostics(response.data); }); }}>检查状态 <ChevronRight size={14} /></button></div>{diagnostics && <div className="diagnostics-summary"><div><span>适配器版本</span><span>{diagnostics.adapterVersion}</span></div><div><span>所选账号 / 进行中的刷新</span><span>{diagnostics.selectedAccountCount} / {diagnostics.activeJobCount}</span></div><div><span>最近错误</span><span>{diagnostics.recentErrorCodes.length ? diagnostics.recentErrorCodes.join("、") : "暂无"}</span></div></div>}</SettingsCard><button className="text-button" disabled={busy !== null} onClick={() => { void perform("position", () => call("window.control", { action: "restore_position" }), "窗口位置已恢复。"); }}>恢复悬浮窗位置</button></>}
      </>}
      <div className="settings-content-footer"><ShieldCheck size={12} /><span>读取凭据与配额均由本机后台处理</span><button className="text-button" onClick={() => { void reload(); }} disabled={busy !== null}>同步状态</button></div>
    </section></div>
  </div>);
}

function AliasInput({ account, reload }: { account: AccountSummary; reload: () => Promise<void> }) {
  const [value, setValue] = useState(account.alias ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const draft = useRef(value);
  const saved = useRef(account.alias ?? "");
  const active = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const composing = useRef(false);
  const mounted = useRef(true);
  const flush = async () => {
    window.clearTimeout(timer.current);
    if (active.current || composing.current || draft.current.trim() === saved.current) return;
    active.current = true;
    if (mounted.current) { setSaving(true); setError(null); }
    let succeeded = false;
    try {
      while (draft.current.trim() !== saved.current) {
        const pending = draft.current.trim();
        await call("accounts.alias.update", { accountId: account.id, alias: pending });
        saved.current = pending;
      }
      await reload();
      succeeded = true;
    } catch (failure) { if (mounted.current) setError(errorMessage(failure)); }
    finally {
      active.current = false;
      if (mounted.current) setSaving(false);
      if (succeeded && draft.current.trim() !== saved.current) void flush();
    }
  };
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; window.clearTimeout(timer.current); void flush(); }; }, []);
  return localize(<div className="alias-editor"><input aria-label={`显示名称 ${account.displayName}`} maxLength={80} value={value} placeholder={account.displayName} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; void flush(); }} onChange={event => { draft.current = event.target.value; setValue(draft.current); window.clearTimeout(timer.current); timer.current = window.setTimeout(() => { void flush(); }, 600); }} onBlur={() => { void flush(); }} onKeyDown={event => { if (event.key === "Enter") void flush(); }} /><span>{error ? <span className="field-error" role="alert">{error}</span> : saving ? "正在保存…" : null}</span></div>);
}

function SectionHeading({ title }: { eyebrow: string; title: string; description: string }) { return localize(<div className="section-heading"><h1>{title}</h1></div>); }
function SettingsCard({ title, description, icon, action, children }: { title: string; description?: string; icon: React.ReactNode; action?: React.ReactNode; children: React.ReactNode }) { return localize(<section className="settings-card"><div className="settings-card-heading"><span>{icon}</span><div><h2>{title}</h2>{description && <p>{description}</p>}</div>{action}</div>{children}</section>); }
function ToggleRow({ title, checked, disabled, onChange }: { title: string; detail: string; checked: boolean; disabled: boolean; onChange: (value: boolean) => void }) { return localize(<div className="setting-row"><div><strong>{title}</strong></div><button className={`toggle ${checked ? "checked" : ""}`} role="switch" aria-checked={checked} aria-label={title} disabled={disabled} onClick={() => onChange(!checked)}><span /></button></div>); }

const sourceNames: Record<string, string> = { official: "官方客户端", cockpit: "Cockpit Tools", ccswitch: "CC Switch", cliproxyapi: "CLIProxyAPI", sub2api: "Sub2API" };
function ProviderIcon({ provider }: { provider: string }) { const name = provider === "codex_usage" || provider === "codex" ? "codex" : provider; return <img className="provider-icon" src={`/providers/${name}.${name === "claude" ? "png" : "svg"}`} alt={name} title={name === "codex" ? "Codex" : name === "claude" ? "Claude" : name === "grok" ? "Grok" : "Antigravity"} />; }
function SourceDialog({ error, sources, setSources, busy, onClose, onPick, onSave }: { error: string | null; sources: SourceOption[]; setSources: React.Dispatch<React.SetStateAction<SourceOption[]>>; busy: string | null; onClose: () => void; onPick: (id: string) => void; onSave: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const dialog = ref.current; dialog?.showModal(); return () => dialog?.close(); }, []);
  return localize(<dialog ref={ref} className="source-dialog" aria-labelledby="source-dialog-title" onCancel={e => { e.preventDefault(); if (!busy) onClose(); }}>
    <div className="dialog-heading"><h2 id="source-dialog-title">选择数据源</h2><button className="icon-button" aria-label="关闭" disabled={!!busy} onClick={onClose}><X size={17} /></button></div>
    {error && <p className="source-error" role="alert">{error}</p>}
    <div className="source-options">{sources.map(source => <div className="source-option" key={source.id}><label><input type="checkbox" checked={source.enabled} disabled={!!busy || !source.path} onChange={e => setSources(current => current.map(s => s.id === source.id ? { ...s, enabled: e.target.checked } : s))} /><strong>{sourceNames[source.id]}</strong></label><button className="secondary-button" disabled={!!busy} onClick={() => onPick(source.id)}>选择目录</button><span className="source-location" title={source.path ?? undefined}>{source.path ?? t("未检测到目录")}</span>{source.id === "sub2api" && <small>本地账号导出 JSON</small>}{source.id === "ccswitch" && <small>OAuth 登录账号</small>}</div>)}</div>
    <div className="dialog-actions"><button className="secondary-button" disabled={!!busy} onClick={onClose}>取消</button><button className="primary-button" disabled={!!busy} onClick={onSave}>{busy === "sources-save" && <LoaderCircle size={14} className="spin" />}保存并扫描</button></div>
  </dialog>);
}
