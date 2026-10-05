import { localize, setLanguage, t } from "./i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AccountAvailability, AccountQuota, AccountSummary, Capabilities, Diagnostics, QuotaWindow, Settings, SettingsPatch, Snapshot, ThemeSummary } from "../contracts/hud-api";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { ArrowDown, ArrowLeft, ArrowUp, Check, ChevronRight, CircleAlert, EyeOff, Database, FilePlus2, Info, Layers3, LoaderCircle, Palette, RefreshCw, Settings2, ShieldCheck, SlidersHorizontal, X } from "lucide-react";
import { call, desktop, errorMessage, hud, internal } from "./api";
import { demoSnapshot } from "./demo";
import { defaultTheme, themeStyle, type ThemeDocument } from "./themes";

interface AppState {
  snapshot: Snapshot | null;
  accounts: AccountSummary[];
  settings: Settings | null;
  themes: ThemeSummary[];
  theme: ThemeDocument;
  sourcePath: string | null;
  capabilities: Capabilities | null;
}

const initialState: AppState = { snapshot: null, accounts: [], settings: null, themes: [], theme: defaultTheme, sourcePath: null, capabilities: null };
const sourceLabels = { ready: "数据源已连接", not_configured: "尚未连接数据源", unavailable: "数据源暂不可用", unsupported: "数据源格式不受支持" };
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
          call("quota.snapshot.get", {}), call("accounts.list", {}), call("settings.get", {}), call("themes.list", {}), call("capabilities.get", {}), !desktop || new URLSearchParams(location.search).get("view") === "settings" ? internal("source_get", {}) : Promise.resolve({ path: null }), internal("theme_get", {}),
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


  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer); }, []);
  useEffect(() => {
    if (!desktop || settingsView) return;
    const panel = document.querySelector<HTMLElement>(".hud-shell");
    if (!panel) return;
    let timer: number | undefined;
    let previousHeight = 0;
    const observer = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const height = Math.max(64, Math.min(560, Math.ceil(panel.getBoundingClientRect().height)));
        if (height !== previousHeight) {
          previousHeight = height;
          void getCurrentWindow().setSize(new LogicalSize(window.innerWidth, height)).catch(() => {});
        }
      }, 80);
    });
    observer.observe(panel);
    return () => { observer.disconnect(); window.clearTimeout(timer); };
  }, [settingsView]);

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
  return localize(<main className={`app-stage ${desktop ? "desktop" : "browser"} ${settingsView ? "settings-stage" : "hud-stage"}`}>
    {settingsView ?
      <SettingsView state={state} loading={loading} error={actionError ?? readError} notice={notice} busy={busy} perform={perform} reload={reload} onBack={() => { setSettingsView(false); history.replaceState(null, "", location.pathname); }} /> :
      <div className="hud-shell" style={themeStyle(state.theme)} data-tauri-drag-region>
        {loading && !state.snapshot ? <div className="hud-empty" data-tauri-drag-region>正在读取配额…</div> : state.snapshot ?
          <HudContent snapshot={state.snapshot} theme={state.theme} now={now} onSettings={openSettings} onRefresh={() => { void perform("refresh-all", () => call("refresh.request", {})); }} onHide={() => { void perform("hide", () => call("window.control", { action: "hide" })); }} busy={busy} /> :
          <div className="hud-empty"><button className="text-button" onClick={() => { void reload(); }}>读取失败 · 点击重试</button></div>}
        {(actionError ?? readError) && <div className="hud-error" role="alert">{actionError ?? readError}</div>}
      </div>}
    {!desktop && !settingsView && <div className="browser-caption"><span>虚构数据预览 · 更多操作位于托盘右键菜单</span><button className="text-button" onClick={openSettings}>预览设置</button></div>}
  </main>);
}

function DemoBanner() { return localize(<div className="demo-banner"><span className="demo-label">DEMO</span><span>浏览器预览 · 全部账号与配额均为虚构</span></div>); }

function HudContent({ snapshot, theme, now, onSettings, onRefresh, onHide, busy }: { snapshot: Snapshot; theme: ThemeDocument; now: number; onSettings?: () => void; onRefresh?: () => void; onHide?: () => void; busy?: string | null }) {
  const hasCredits = snapshot.quotas.some(quota => quota.resetCreditsAvailable != null);
  return localize(<div className="hud-body" data-tauri-drag-region>
    {snapshot.accounts.length ? <>
      <section className={`compact-accounts ${hasCredits ? "has-credits" : ""}`} aria-label="账号配额" data-tauri-drag-region>{snapshot.accounts.map(account => <AccountRow key={account.id} account={account} quota={snapshot.quotas.find(quota => quota.accountId === account.id)} availability={snapshot.availability.find(item => item.accountId === account.id)} now={now} showCredits={hasCredits} interactive={!!onSettings} />)}</section>
    </> : <div className="hud-empty" data-tauri-drag-region><span>{snapshot.source.state === "ready" ? "尚未选择账号" : "尚未连接账号来源"}</span><span className="hud-empty-hint">右键托盘 → 设置</span></div>}
    <RecommendationStrip snapshot={snapshot} now={now} onSettings={onSettings} onRefresh={onRefresh} onHide={onHide} busy={busy} />
    <span className="sr-only">{theme.name}</span>
  </div>);
}

function AccountRow({ account, quota, availability, now, showCredits, interactive }: { account: AccountSummary; quota?: AccountQuota; availability?: AccountAvailability; now: number; showCredits: boolean; interactive: boolean }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setError(null); }, [quota?.lastSuccessAt]);
  const refreshing = pending || quota?.status === "refreshing";
  const status = error ?? statusText(quota, availability);
  const state = error || quota?.error ? "error" : quota?.freshness === "stale" ? "stale" : availability?.state ?? "unknown";
  const windows = [18000, 604800].map(duration => quota?.windows.find(window => window.scope === "base" && window.durationSeconds === duration));
  const refresh = async () => {
    setPending(true); setError(null);
    try { await call("refresh.request", { accountIds: [account.id] }); }
    catch (failure) { setError(errorMessage(failure)); }
    finally { setPending(false); }
  };
  return localize(<div className={`compact-row state-${state}`} data-tauri-drag-region>
    <button className={`row-refresh ${refreshing ? "refreshing" : ""}`} aria-label={`刷新 ${account.displayName}`} disabled={!interactive || refreshing} onClick={() => { void refresh(); }}><RefreshCw size={13} className={refreshing ? "spin" : ""} /></button>
    <span className="compact-name" translate="no" data-tauri-drag-region>{account.displayName}</span>
    {windows.map((window, index) => <span className={`compact-quota ${quotaBand(window)}`} key={index} aria-label={`${index === 0 ? "5h" : "7d"} 剩余 ${window ? remainingText(window) : "未知"}，${window ? durationText(window.resetsAt, now) : "待查询"}`} data-tauri-drag-region><strong>{window ? remainingText(window) : "—"}</strong><span className="quota-dot">·</span><span>{window ? durationText(window.resetsAt, now) : "—"}</span></span>)}
    {showCredits && <span className="reset-credits" aria-label={quota?.resetCreditsAvailable != null ? `可用重置次数 ${quota.resetCreditsAvailable}` : "重置次数未知"} data-tauri-drag-region>{quota?.resetCreditsAvailable != null ? `R: ${quota.resetCreditsAvailable}` : ""}</span>}
    {state === "error" && <span className="row-state state-error" role="img" aria-label={status}>!</span>}
  </div>);
}

function quotaBand(window?: QuotaWindow): string {
  if (window?.measurement !== "percent" || window.remainingPercent === null) return "";
  const percent = window.remainingPercent;
  return percent < 20 ? "quota-red" : percent < 50 ? "quota-orange" : percent < 80 ? "quota-blue" : "quota-green";
}

function RecommendationStrip({ snapshot, now, onSettings, onRefresh, onHide, busy }: { snapshot: Snapshot; now: number; onSettings?: () => void; onRefresh?: () => void; onHide?: () => void; busy?: string | null }) {
  const recommendation = snapshot.recommendation;
  const account = snapshot.accounts.find(item => item.id === recommendation.accountId);
  const refreshing = busy === "refresh-all" || snapshot.quotas.some(quota => quota.status === "refreshing");
  return localize(<div className={`compact-recommendation state-${recommendation.state}`} data-tauri-drag-region>
    <span>avaliable:</span><strong translate="no" data-tauri-drag-region>{account?.displayName ?? "—"}</strong><span>·</span><span className="usable-time">{recommendation.state === "now" ? "now" : recommendation.state === "waiting" ? durationText(recommendation.estimatedAvailableAt, now) : "—"}</span>
    <div className="hud-actions">
      <button className="row-refresh" aria-label="设置" disabled={!onSettings || !!busy} onClick={onSettings}><Settings2 size={13} /></button>
      <button className="row-refresh" aria-label="刷新全部" disabled={!onRefresh || !!busy || refreshing || !snapshot.accounts.length} onClick={onRefresh}><RefreshCw size={13} className={refreshing ? "spin" : ""} /></button>
      <button className="row-refresh" aria-label="隐藏悬浮窗" disabled={!onHide || !!busy} onClick={onHide}><EyeOff size={13} /></button>
    </div>
  </div>);
}

type Performer = (key: string, action: () => Promise<unknown>, success?: string) => Promise<void>;
type SettingSection = "general" | "accounts" | "themes" | "about";

function SettingsView({ state, loading, error, notice, busy, perform, reload, onBack }: { state: AppState; loading: boolean; error: string | null; notice: string | null; busy: string | null; perform: Performer; reload: () => Promise<void>; onBack: () => void }) {
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const [section, setSection] = useState<SettingSection>("general");
  const [selection, setSelection] = useState<string[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [preview, setPreview] = useState<ThemeDocument | null>(null);
  const [imported, setImported] = useState<ThemeSummary | null>(null);
  const [interval, setIntervalValue] = useState("300");
  const savedSelection = JSON.stringify(state.accounts.filter(account => account.selected).sort((a, b) => a.order - b.order).map(account => account.id));
  useEffect(() => { setSelection(JSON.parse(savedSelection) as string[]); }, [savedSelection]);
  useEffect(() => { if (state.settings) setIntervalValue(String(state.settings.refreshIntervalSeconds)); }, [state.settings?.refreshIntervalSeconds]);
  const settings = state.settings;
  const patch = (key: string, fields: Omit<SettingsPatch, "expectedRevision">) => { if (settings) void perform(key, () => call("settings.update", { expectedRevision: settings.settingsRevision, ...fields })); };
  const chooseSource = () => { void perform("choose-source", () => internal("source_choose", {})); };
  const rescan = () => { void perform("rescan", () => internal("source_rescan", {}), "账号列表已重新读取。"); };
  const selectTheme = (id: string) => { if (settings) void perform("select-theme", async () => { await call("themes.select", { id, expectedRevision: settings.settingsRevision }); setPreview(null); setImported(null); }, "主题已应用。"); };
  const previewTheme = (id: string) => { void perform("preview-theme", async () => { const document = await internal("theme_get", { id }); await call("themes.preview", { document }); setPreview(document); }); };
  const importTheme = () => { void perform("import-theme", async () => { const response = await call("themes.import", {}); if (response.data.cancelled || !response.data.theme) return; const theme = response.data.theme; setImported(theme); const document = await internal("theme_get", { id: theme.id }); await call("themes.preview", { document }); setPreview(document); }); };
  const changeOrder = (id: string, direction: number) => setSelection(current => { const index = current.indexOf(id); const destination = index + direction; if (index < 0 || destination < 0 || destination >= current.length) return current; const next = [...current]; [next[index], next[destination]] = [next[destination], next[index]]; return next; });
  const selectionDirty = JSON.stringify(selection) !== JSON.stringify(state.accounts.filter(account => account.selected).sort((a, b) => a.order - b.order).map(account => account.id));
  const nav = [{ id: "general", label: "常规", icon: <SlidersHorizontal size={17} /> }, { id: "accounts", label: "账号", icon: <Database size={17} /> }, { id: "themes", label: "外观", icon: <Palette size={17} /> }, { id: "about", label: "关于", icon: <Info size={17} /> }] as const;

  return localize(<div className="settings-shell" style={themeStyle(defaultTheme)}>
    <header className="settings-header" data-tauri-drag-region><div className="brand" data-tauri-drag-region><div className="brand-icon"><Layers3 size={19} /></div><span className="brand-name" data-tauri-drag-region>Chatgpt HUD <span className="brand-subtle">/ 设置</span></span></div><div className="header-actions">{!desktop && <button className="text-button return-button" onClick={onBack}><ArrowLeft size={14} />返回 HUD</button>}</div></header>
    {!desktop && <DemoBanner />}
    <div className="settings-layout"><aside className="settings-sidebar"><div className="sidebar-kicker">WORKSPACE</div><nav aria-label="设置分区">{nav.map(item => <button key={item.id} className={`nav-button ${section === item.id ? "selected" : ""}`} onClick={() => setSection(item.id)}>{item.icon}<span>{item.label}</span><ChevronRight size={13} /></button>)}</nav><div className="sidebar-footer"><ShieldCheck size={15} /><span>凭据只留在本机</span><small>v0.2.0</small></div></aside>
    <section className="settings-content">
      {exportNotice && <div className="settings-alert success" role="status">{exportNotice}</div>}
      {(error || notice) && <div className={error ? "settings-alert error" : "settings-alert success"} role={error ? "alert" : "status"}>{error ? <CircleAlert size={16} /> : <Check size={16} />}<span>{error ?? notice}</span></div>}
      {loading && !settings ? <div className="empty-state"><LoaderCircle size={24} className="spin" /><span>正在读取设置</span></div> : <>
      {section === "general" && <><SectionHeading eyebrow="PREFERENCES" title="常规" description="连接你的账号，调整刷新与窗口行为。" />
        <SettingsCard title="语言" icon={<Settings2 size={17} />}><div className="setting-row"><div><strong>界面语言</strong><span>更改后立即保存</span></div><select aria-label="界面语言" value={settings?.display.locale === "zh-CN" ? "zh-CN" : "en"} disabled={busy !== null || !settings} onChange={event => patch("language", { display: { locale: event.target.value as "en" | "zh-CN" } })}><option value="en">English</option><option value="zh-CN">简体中文</option></select></div></SettingsCard>
        <SettingsCard title="数据源" description="读取官方客户端或 Cockpit Tools 登录账号" icon={<Database size={17} />}><div className="source-picker"><div><span className={`source-badge ${state.snapshot?.source.state === "ready" ? "connected" : ""}`}><span className="status-dot" />{state.snapshot ? sourceLabels[state.snapshot.source.state] : "未连接"}</span><span className="source-path" title={state.sourcePath ?? undefined}>{state.sourcePath ?? "选择官方账号或 Cockpit 数据目录"}</span>{state.snapshot?.source.error && <span className="field-error">{t("数据源暂不可用")} ({state.snapshot.source.error.code})</span>}</div><button className="secondary-button" disabled={busy !== null} onClick={chooseSource}>{busy === "choose-source" ? <LoaderCircle size={14} className="spin" /> : <Database size={14} />}选择目录</button></div><p className="source-help">默认优先读取 CODEX_HOME/auth.json（通常为 ~/.codex/auth.json）。系统凭据库或仅内存登录暂不支持。</p><div className="card-bottom"><span>只读 auth.json 或 Cockpit 账号；请在原客户端登录。</span><button className="text-button" onClick={rescan} disabled={busy !== null}><RefreshCw size={13} className={busy === "rescan" ? "spin" : ""} />重新扫描</button></div></SettingsCard>
        <SettingsCard title="配额刷新" icon={<RefreshCw size={17} />}><ToggleRow title="自动刷新" detail="定期获取所选账号的最新配额" checked={settings?.autoRefresh ?? false} disabled={busy !== null || !settings} onChange={checked => patch("auto-refresh", { autoRefresh: checked })} /><div className="setting-row"><div><strong>刷新间隔</strong><span>手动刷新与自动刷新共用调度器</span></div><select aria-label="刷新间隔" value={interval} disabled={busy !== null || !settings} onChange={event => { setIntervalValue(event.target.value); patch("interval", { refreshIntervalSeconds: Number(event.target.value) }); }}><option value="60">1 分钟</option><option value="300">5 分钟</option><option value="600">10 分钟</option><option value="900">15 分钟</option><option value="1800">30 分钟</option>{![60, 300, 600, 900, 1800].includes(settings?.refreshIntervalSeconds ?? 300) && <option value={settings?.refreshIntervalSeconds}>{settings?.refreshIntervalSeconds} 秒</option>}</select></div></SettingsCard>
        <SettingsCard title="窗口与显示" icon={<Settings2 size={17} />}><ToggleRow title="始终置顶" detail="让 HUD 保持在其他窗口上方" checked={settings?.display.alwaysOnTop ?? true} disabled={busy !== null || !settings} onChange={checked => patch("pin", { display: { alwaysOnTop: checked } })} /><ToggleRow title="隐私模式" detail="用“账号 1”等名称遮罩账号显示" checked={settings?.display.privacyMode ?? false} disabled={busy !== null || !settings} onChange={checked => patch("privacy", { display: { privacyMode: checked } })} /></SettingsCard>
      </>}
      {section === "accounts" && <><SectionHeading eyebrow="YOUR ACCOUNTS" title="账号" description="勾选要显示的账号，并调整 HUD 中的顺序。" />
        <div className="account-selection-header"><span>{selection.length} 个已选择 / {state.accounts.length} 个账号</span><button className="text-button" disabled={busy !== null} onClick={rescan}><RefreshCw size={14} className={busy === "rescan" ? "spin" : ""} />重新扫描</button></div>
        <div className="selection-list">{state.accounts.length ? [...state.accounts].sort((a, b) => { const ai = selection.indexOf(a.id); const bi = selection.indexOf(b.id); return (ai < 0 ? 1000 + a.order : ai) - (bi < 0 ? 1000 + b.order : bi); }).map(account => <div className={`selection-row ${selection.includes(account.id) ? "chosen" : ""}`} key={account.id}><label><input type="checkbox" checked={selection.includes(account.id)} disabled={busy !== null || account.support === "unsupported"} onChange={event => setSelection(current => event.target.checked ? [...current, account.id] : current.filter(id => id !== account.id))} /><span className="selection-check"><Check size={12} /></span><span className="account-avatar">{account.displayName.slice(0, 1).toUpperCase()}</span><span className="selection-identity"><strong translate="no">{account.displayName}</strong><span>{["codex", "codex_usage"].includes(account.providerId) ? "Codex" : account.providerId} · {account.support === "unsupported" ? "暂不支持" : account.support === "unknown" ? "支持情况待核实" : account.isCurrent ? "当前本机登录" : "已发现"}</span></span></label><AliasInput account={account} reload={reload} />{selection.includes(account.id) && <div className="order-actions"><button className="icon-button" aria-label={`上移 ${account.displayName}`} disabled={busy !== null || selection.indexOf(account.id) === 0} onClick={() => changeOrder(account.id, -1)}><ArrowUp size={14} /></button><button className="icon-button" aria-label={`下移 ${account.displayName}`} disabled={busy !== null || selection.indexOf(account.id) === selection.length - 1} onClick={() => changeOrder(account.id, 1)}><ArrowDown size={14} /></button></div>}</div>) : <div className="empty-state"><Database size={24} /><strong>尚未发现账号</strong><span>选择含 auth.json 或 codex_accounts.json 的目录，再重新扫描。</span><button className="secondary-button" disabled={busy !== null} onClick={chooseSource}>选择数据目录</button></div>}</div>
        <div className="form-actions"><span>最多显示 {state.capabilities?.maxRefreshAccounts ?? 100} 个账号</span><button className="primary-button" disabled={busy !== null || !settings || !selectionDirty || selection.length > (state.capabilities?.maxRefreshAccounts ?? 100)} onClick={() => { if (settings) void perform("selection", () => call("accounts.selection.update", { expectedRevision: settings.settingsRevision, accountIds: selection }), "账号选择已保存。"); }}>{busy === "selection" ? <LoaderCircle size={14} className="spin" /> : <Check size={14} />}保存选择</button></div>
      </>}
      {section === "themes" && <><SectionHeading eyebrow="MAKE IT YOURS" title="外观" description="选择内置主题，或导入自己编写的主题文件。" />
        <div className="themes-grid">{state.themes.map(theme => <div className={`theme-option ${settings?.activeThemeId === theme.id ? "theme-active" : ""}`} key={theme.id}><button className={`theme-swatch swatch-${theme.id === "paper" ? "paper" : theme.id === "midnight" ? "midnight" : "default"}`} aria-label={`预览 ${theme.name}`} disabled={busy !== null} onClick={() => previewTheme(theme.id)}><div className="mini-header"><span /><span /><span /></div>{[0, 1, 2].map(index => <div className="mini-row" key={index}><span /><span /><span /></div>)}<div className="mini-recommendation" /></button><div className="theme-option-info"><span><strong>{theme.name}</strong><small>{theme.builtIn ? "内置主题" : "自定义主题"}</small></span><button className={`theme-apply ${settings?.activeThemeId === theme.id ? "selected" : ""}`} disabled={busy !== null || settings?.activeThemeId === theme.id} aria-label={settings?.activeThemeId === theme.id ? `${theme.name} 已应用` : `应用 ${theme.name}`} onClick={() => selectTheme(theme.id)}>{settings?.activeThemeId === theme.id ? <Check size={16} /> : "应用"}</button></div></div>)}</div>
        <div className="theme-import-card"><div className="import-icon"><FilePlus2 size={21} /></div><div><strong>添加自己的主题</strong><p>用 theme.json 定制颜色、字体与布局</p></div><button className="secondary-button" disabled={busy !== null} onClick={importTheme}>{busy === "import-theme" ? <LoaderCircle size={14} className="spin" /> : <FilePlus2 size={14} />}导入主题</button></div>
        {preview && <div className="theme-preview-section"><div className="preview-heading"><span><span className="demo-label">PREVIEW</span>{preview.name} · 虚构数据</span><button className="icon-button" aria-label="关闭主题预览" onClick={() => { setPreview(null); setImported(null); }}><X size={15} /></button></div><div className="embedded-hud" style={themeStyle(preview)}><HudContent snapshot={demoSnapshot()} theme={preview} now={Date.now()} /></div><div className="form-actions"><span>主题只改变外观，不改变配额判断</span><button className="primary-button" disabled={busy !== null} onClick={() => selectTheme(imported?.id ?? preview.id)}>应用此主题 <Check size={14} /></button></div></div>}
        <p className="helper-note"><ShieldCheck size={14} />主题仅接受本地 JSON，导入前会校验格式与可读性。<button className="text-button" disabled={busy !== null} onClick={() => { if (desktop) void perform("export-theme", async () => { const result = await internal("theme_export", {}); if (!result.cancelled) setExportNotice(t("主题示例已保存。")); }); else { const link = document.createElement("a"); link.href = "/theme-template.json"; link.download = "theme.json"; link.click(); } }}>下载示例</button></p>
      </>}
      {section === "about" && <><SectionHeading eyebrow="SMALL FOOTPRINT" title="专注配额，轻量常驻。" description="Tauri 2 + Rust · 开放接口 · 可自行扩展主题" /><SettingsCard title="Chatgpt HUD" description="0.2.0" icon={<Layers3 size={18} />}><p className="about-description">读取官方客户端或 Cockpit Tools 登录账号，展示配额与重置时间。认证失效请在原客户端重新登录。</p><div className="about-status"><span className="status-dot" />{desktop ? "正在桌面应用中运行" : "浏览器演示，全部数据均为虚构"}</div></SettingsCard><SettingsCard title="开放能力" description="以当前应用实际提供的方法为准" icon={<SlidersHorizontal size={17} />} action={<button className="text-button" onClick={() => { if (desktop) void perform("docs", () => internal("docs_open", {})); else window.open("/api.html", "_blank", "noopener,noreferrer"); }}>接口说明 <ChevronRight size={14} /></button>}><div className="capability-summary"><div><strong>{state.capabilities?.enabledMethods.length ?? 0}</strong><span>可用方法</span></div><div><strong>{state.capabilities?.apiVersion ?? "—"}</strong><span>接口版本</span></div><div><strong>{state.capabilities?.themeSchemaVersions.join(", ") ?? "—"}</strong><span>主题版本</span></div></div><div className="card-bottom"><span>第一版通过应用内部接口调用。</span><button className="text-button" disabled={busy !== null || !state.capabilities?.enabledMethods.includes("diagnostics.get")} onClick={() => { void perform("diagnostics", async () => { const response = await call("diagnostics.get", {}); setDiagnostics(response.data); }); }}>检查状态 <ChevronRight size={14} /></button></div>{diagnostics && <div className="diagnostics-summary"><div><span>适配器版本</span><span>{diagnostics.adapterVersion}</span></div><div><span>所选账号 / 进行中的刷新</span><span>{diagnostics.selectedAccountCount} / {diagnostics.activeJobCount}</span></div><div><span>最近错误</span><span>{diagnostics.recentErrorCodes.length ? diagnostics.recentErrorCodes.join("、") : "暂无"}</span></div></div>}</SettingsCard><button className="text-button" disabled={busy !== null} onClick={() => { void perform("position", () => call("window.control", { action: "restore_position" }), "窗口位置已恢复。"); }}>恢复悬浮窗位置</button></>}
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
  return localize(<div className="alias-editor"><input aria-label={`显示名称 ${account.displayName}`} maxLength={80} value={value} placeholder={account.displayName} onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; void flush(); }} onChange={event => { draft.current = event.target.value; setValue(draft.current); window.clearTimeout(timer.current); timer.current = window.setTimeout(() => { void flush(); }, 600); }} onBlur={() => { void flush(); }} onKeyDown={event => { if (event.key === "Enter") void flush(); }} /><span>{error ? <span className="field-error" role="alert">{error}</span> : saving ? "正在保存…" : "显示名称"}</span></div>);
}

function SectionHeading({ eyebrow, title, description }: { eyebrow: string; title: string; description: string }) { return localize(<div className="section-heading"><span>{eyebrow}</span><h1>{title}</h1><p>{description}</p></div>); }
function SettingsCard({ title, description, icon, action, children }: { title: string; description?: string; icon: React.ReactNode; action?: React.ReactNode; children: React.ReactNode }) { return localize(<section className="settings-card"><div className="settings-card-heading"><span>{icon}</span><div><h2>{title}</h2>{description && <p>{description}</p>}</div>{action}</div>{children}</section>); }
function ToggleRow({ title, detail, checked, disabled, onChange }: { title: string; detail: string; checked: boolean; disabled: boolean; onChange: (value: boolean) => void }) { return localize(<div className="setting-row"><div><strong>{title}</strong><span>{detail}</span></div><button className={`toggle ${checked ? "checked" : ""}`} role="switch" aria-checked={checked} aria-label={title} disabled={disabled} onClick={() => onChange(!checked)}><span /></button></div>); }
