import type { AccountQuota, AccountSummary, ApiResult, HudEvent, MethodMap, RefreshJob, Settings, Snapshot, SourceStatus, ThemeSummary, ThemeValidation } from "../contracts/hud-api";
import { defaultTheme, midnightTheme, paperTheme, type ThemeDocument } from "./themes";

const instanceId = "browser-demo-only";
let revision = 1;
let sequence = 0;
const listeners = new Set<(event: HudEvent) => void>();
const source: SourceStatus = { state: "ready", format: "plaintext", adapterVersion: "演示", error: null };
const themes = new Map<string, ThemeDocument>([defaultTheme, midnightTheme, paperTheme].map(theme => [theme.id, theme]));
let settings: Settings = { settingsRevision: 1, refreshIntervalSeconds: 300, autoRefresh: true, display: { alwaysOnTop: true, showHoverDetails: false, privacyMode: false, locale: "en" }, activeThemeId: "default" };
let accounts: AccountSummary[] = [
  { id: "demo-atlas", displayName: "atlas", providerId: "codex", selected: true, order: 0, support: "supported" },
  { id: "demo-studio", displayName: "studio", providerId: "codex", selected: true, order: 1, support: "supported" },
  { id: "demo-work", displayName: "work", providerId: "codex", selected: true, order: 2, support: "supported" },
  { id: "demo-personal", displayName: "personal", providerId: "codex", selected: false, order: 3, support: "supported" },
];
let lastSuccess = new Date(Date.now() - 45000).toISOString();
const refreshing = new Set<string>();
const jobs = new Map<string, RefreshJob>();
const resets = [35 * 60, 8 * 3600, 3 * 86400, 5 * 86400];
const baseTime = Date.now();

function result<T>(data: T): ApiResult<T> {
  return { apiVersion: "1.0", instanceId, revision, requestId: crypto.randomUUID(), generatedAt: new Date().toISOString(), ok: true, data };
}

function fail(code: "INVALID_ARGUMENT" | "CONFLICT" | "THEME_INVALID" | "NOT_FOUND", message: string): ApiResult<never> {
  return { apiVersion: "1.0", instanceId, revision, requestId: crypto.randomUUID(), generatedAt: new Date().toISOString(), ok: false, error: { code, message, retryAt: null, retryable: false } };
}

function emit(type: HudEvent["type"], data: HudEvent["data"] = {}) {
  revision += 1;
  const event: HudEvent = { apiVersion: "1.0", instanceId, revision, sequence: ++sequence, emittedAt: new Date().toISOString(), type, data };
  listeners.forEach(listener => listener(event));
}

export function demoSubscribe(listener: (event: HudEvent) => void): () => void {
  listeners.add(listener);
  queueMicrotask(() => listener({ apiVersion: "1.0", instanceId, revision, sequence: ++sequence, emittedAt: new Date().toISOString(), type: "resync.required", data: {} }));
  return () => { listeners.delete(listener); };
}

export function demoSnapshot(): Snapshot {
  const selected = accounts.filter(account => account.selected).sort((a, b) => a.order - b.order).map((account, index) => ({ ...account, displayName: settings.display.privacyMode ? `${settings.display.locale === "zh-CN" ? "账号" : "Account"} ${index + 1}` : account.displayName }));
  const quotas: AccountQuota[] = selected.map(account => {
    const originalIndex = accounts.findIndex(item => item.id === account.id);
    const old = account.id === "demo-work";
    return {
      accountId: account.id, origin: old ? "cockpit_cache" : "network", freshness: old ? "stale" : "fresh",
      status: refreshing.has(account.id) ? "refreshing" : "ok", lastSuccessAt: old ? new Date(baseTime - 25 * 60000).toISOString() : lastSuccess,
      lastAttemptAt: lastSuccess, observedAt: lastSuccess, validUntil: new Date(Date.now() + 300000).toISOString(), providerAllowed: true, baseCoverageComplete: true, blockingReason: "none", error: null,
      resetCreditsAvailable: [1, 0, null, 2][originalIndex],
      windows: [
        { id: "primary", scope: "base", kind: "primary", label: "5h", durationSeconds: 18000, applicability: "required", measurement: "percent", remainingPercent: [74, 1, 28, 92][originalIndex], exhausted: false, resetsAt: new Date(baseTime + resets[originalIndex % 2] * 1000).toISOString() },
        { id: "secondary", scope: "base", kind: "secondary", label: "7d", durationSeconds: 604800, applicability: "required", measurement: "percent", remainingPercent: [61, 42, 83, 98][originalIndex], exhausted: false, resetsAt: new Date(baseTime + resets[2 + originalIndex % 2] * 1000).toISOString() },
      ],
    };
  });
  const available = selected.find(account => account.id !== "demo-studio" && account.id !== "demo-work");
  const waiting = selected.find(account => account.id === "demo-studio");
  return {
    source,
    accounts: selected, quotas,
    availability: selected.map(account => account.id === "demo-work"
      ? { accountId: account.id, state: "unknown", reason: "stale", estimatedAvailableAt: null }
      : account.id === "demo-studio"
        ? { accountId: account.id, state: "waiting", reason: "below_threshold", estimatedAvailableAt: new Date(baseTime + resets[1] * 1000).toISOString() }
        : { accountId: account.id, state: "now", reason: "available", estimatedAvailableAt: null }),
    recommendation: { state: available ? "now" : waiting ? "waiting" : selected.length ? "unknown" : "empty", accountId: available?.id ?? waiting?.id ?? null, estimatedAvailableAt: available ? null : waiting ? new Date(baseTime + resets[1] * 1000).toISOString() : null, reason: available ? "available" : waiting ? "earliest_available" : "no_data", coverage: { selected: selected.length, known: selected.filter(account => account.id !== "demo-work").length, unknown: selected.filter(account => account.id === "demo-work").length, complete: !selected.some(account => account.id === "demo-work") } },
    nextRefreshAt: settings.autoRefresh ? new Date(Date.now() + settings.refreshIntervalSeconds * 1000).toISOString() : null,
  };
}

const methods = ["capabilities.get", "accounts.list", "accounts.alias.update", "accounts.selection.update", "quota.snapshot.get", "recommendation.get", "refresh.request", "refresh.status.get", "settings.get", "settings.update", "themes.list", "themes.validate", "themes.preview", "themes.import", "themes.select", "window.control", "diagnostics.get"];

export async function demoCall<M extends keyof MethodMap>(method: M, params: MethodMap[M]["params"]): Promise<ApiResult<MethodMap[M]["result"]>> {
  const request = params as Record<string, unknown>;
  let response: ApiResult<unknown>;
  if (["settings.update", "themes.select", "accounts.selection.update"].includes(method) && request.expectedRevision !== settings.settingsRevision) return fail("CONFLICT", "设置已更新，请重试。") as ApiResult<MethodMap[M]["result"]>;
  switch (method) {
    case "capabilities.get": response = result({ appVersion: "0.2.0", apiVersion: "1.0", transport: "tauri", enabledMethods: methods, grantedScopes: ["quota.read", "quota.refresh", "events.read", "settings.read", "settings.write", "themes.read", "themes.write", "window.control", "diagnostics.read"], themeSchemaVersions: [1], providerIds: ["codex"], maxRefreshAccounts: 100 }); break;
    case "accounts.list": response = result(accounts.map((account, index) => ({ ...account, displayName: settings.display.privacyMode ? `${settings.display.locale === "zh-CN" ? "账号" : "Account"} ${index + 1}` : account.displayName }))); break;
    case "quota.snapshot.get": response = result(demoSnapshot()); break;
    case "recommendation.get": response = result(demoSnapshot().recommendation); break;
    case "settings.get": response = result(settings); break;
    case "settings.update": {
      settings = { ...settings, ...(typeof request.autoRefresh === "boolean" ? { autoRefresh: request.autoRefresh } : {}), ...(typeof request.refreshIntervalSeconds === "number" ? { refreshIntervalSeconds: request.refreshIntervalSeconds } : {}), display: { ...settings.display, ...(request.display as object ?? {}) }, settingsRevision: settings.settingsRevision + 1 };
      emit("settings.changed"); response = result(settings); break;
    }
    case "accounts.selection.update": {
      const ids = request.accountIds as string[];
      if (!Array.isArray(ids) || new Set(ids).size !== ids.length || ids.some(id => !accounts.some(account => account.id === id))) { response = fail("INVALID_ARGUMENT", "账号选择无效。"); break; }
      accounts = accounts.map(account => ({ ...account, selected: ids.includes(account.id), order: ids.indexOf(account.id) >= 0 ? ids.indexOf(account.id) : accounts.length }));
      settings = { ...settings, settingsRevision: settings.settingsRevision + 1 };
      emit("settings.changed"); emit("snapshot.changed"); response = result(settings); break;
    }
    case "refresh.request": {
      const ids = request.accountIds as string[] | undefined ?? accounts.filter(account => account.selected).map(account => account.id);
      if (!ids.length) { response = fail("INVALID_ARGUMENT", "请先选择要显示的账号。"); break; }
      const active = Array.from(jobs.values()).find(job => job.state === "running" && job.items.length === ids.length && job.items.every(item => ids.includes(item.accountId)));
      if (active) { response = result({jobId: active.jobId, joined:true}); break; }
      const job: RefreshJob = { jobId: crypto.randomUUID(), state: "running", createdAt: new Date().toISOString(), finishedAt: null, items: ids.map(accountId => ({ accountId, state: "running", error: null })) };
      jobs.set(job.jobId, job);
      ids.forEach(id => refreshing.add(id));
      emit("refresh.progress", {jobId: job.jobId});
      setTimeout(() => {
        ids.forEach(id => refreshing.delete(id)); lastSuccess = new Date().toISOString();
        jobs.set(job.jobId, {...job, state:"completed", finishedAt:lastSuccess, items:job.items.map(item => ({...item,state:"succeeded"}))});
        emit("refresh.completed", {jobId:job.jobId}); emit("snapshot.changed");
      }, 1200);
      response = result({jobId:job.jobId, joined:false}); break;
    }
    case "refresh.status.get": response = jobs.has(request.jobId as string) ? result(jobs.get(request.jobId as string)) : fail("NOT_FOUND", "刷新任务不存在。"); break;
    case "themes.list": response = result(Array.from(themes.values(), theme => ({ id: theme.id, name: theme.name, builtIn: ["default", "midnight", "paper"].includes(theme.id) } satisfies ThemeSummary))); break;
    case "themes.validate": response = result(validateDemoTheme(request.document)); break;
    case "themes.preview": response = validateDemoTheme(request.document).valid ? result({ previewId: crypto.randomUUID() }) : fail("THEME_INVALID", "主题格式无效。"); break;
    case "themes.import": {
      const file = await chooseJson();
      if (!file) { response = result({ cancelled: true, theme: null }); break; }
      if (file.size > 65536) { response = fail("THEME_INVALID", "主题文件不能超过 64 KB。"); break; }
      try {
        const document: unknown = JSON.parse(await file.text());
        const validation = validateDemoTheme(document);
        if (!validation.valid) { response = fail("THEME_INVALID", validation.issues[0].message); break; }
        const theme = document as ThemeDocument;
        if (["default", "midnight", "paper"].includes(theme.id) || themes.has(theme.id)) { response = fail("THEME_INVALID", "主题 ID 已存在，请使用新的 ID。"); break; }
        themes.set(theme.id, theme); response = result({ cancelled: false, theme: { id: theme.id, name: theme.name, builtIn: false } });
      } catch { response = fail("THEME_INVALID", "请导入有效的 JSON 主题文件。"); }
      break;
    }
    case "themes.select": {
      if (!themes.has(request.id as string)) { response = fail("NOT_FOUND", "主题不存在。"); break; }
      settings = { ...settings, activeThemeId: request.id as string, settingsRevision: settings.settingsRevision + 1 }; emit("theme.changed"); response = result(settings); break;
    }
    case "accounts.alias.update": {
      const { accountId, alias } = params as MethodMap["accounts.alias.update"]["params"];
      const account = accounts.find(a => a.id === accountId);
      if (!account || alias.length > 80) return fail("INVALID_ARGUMENT", "Invalid display name") as ApiResult<MethodMap[M]["result"]>;
      account.alias = alias.trim() || null; account.displayName = account.alias ?? account.id.replace("demo-", "");
      settings.settingsRevision++; emit("snapshot.changed"); response = result(settings); break;
    }
    case "window.control": response = result({ accepted: true }); break;
    case "diagnostics.get": response = result({ appVersion: "0.2.0", adapterVersion: "演示", source, selectedAccountCount: accounts.filter(account => account.selected).length, activeJobCount: refreshing ? 1 : 0, recentErrorCodes: [] }); break;
    default: response = fail("INVALID_ARGUMENT", "此功能尚未提供。");
  }
  return response as ApiResult<MethodMap[M]["result"]>;
}

export async function demoInternal(method: string, request: object): Promise<ApiResult<unknown>> {
  switch (method) {
    case "source_get": return result({ path: "浏览器演示 · 虚构账号" });
    case "source_choose": emit("source.changed"); return result({ cancelled: false, path: "浏览器演示 · 虚构账号", source });
    case "source_rescan": emit("source.changed"); return result(source);
    case "theme_get": return result(themes.get((request as { id?: string }).id ?? settings.activeThemeId) ?? defaultTheme);
    default: return fail("INVALID_ARGUMENT", "此功能尚未提供。");
  }
}

function chooseJson(): Promise<File | null> {
  return new Promise(resolve => {
    const input = document.createElement("input"); input.type = "file"; input.accept = ".json,application/json";
    input.onchange = () => { resolve(input.files?.[0] ?? null); input.remove(); };
    input.oncancel = () => { resolve(null); input.remove(); };
    input.style.display = "none"; document.body.append(input); input.click();
  });
}

/** The desktop host performs authoritative schema, contrast and storage checks. */
export function validateDemoTheme(value: unknown): ThemeValidation {
  const issues: ThemeValidation["issues"] = [];
  const issue = (path: string, message: string) => issues.push({ path, code: "INVALID", message });
  const object = (item: unknown): item is Record<string, unknown> => !!item && typeof item === "object" && !Array.isArray(item);
  const keys = (item: Record<string, unknown>, required: string[], optional: string[] = [], path = "") => {
    for (const key of required) if (!(key in item)) issue(`${path}.${key}`, `主题缺少 ${key} 字段。`);
    for (const key of Object.keys(item)) if (![...required, ...optional].includes(key)) issue(`${path}.${key}`, `主题包含未支持的字段 ${key}。`);
  };
  if (!object(value)) return { valid: false, issues: [{ path: "", code: "INVALID", message: "主题必须为 JSON 对象。" }] };
  keys(value, ["schemaVersion", "minHudVersion", "id", "name", "tokens", "layout"], ["author", "description"]);
  if (value.schemaVersion !== 1) issue("schemaVersion", "主题格式版本不受支持。");
  if (typeof value.minHudVersion !== "string" || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value.minHudVersion) || value.minHudVersion.split(".").some((v, index) => Number(v) > [0, 1, 0][index] && value.minHudVersion!.toString().split(".").slice(0, index).every((part, i) => Number(part) === [0, 1, 0][i]))) issue("minHudVersion", "主题需要更新版本的应用，或最低版本格式无效。");
  if (typeof value.id !== "string" || !/^[a-z][a-z0-9-]{0,63}$/.test(value.id)) issue("id", "主题 ID 应使用小写字母、数字和连字符。");
  if (typeof value.name !== "string" || !value.name.trim() || value.name.length > 80) issue("name", "主题名称应为 1–80 个字符。");
  if (value.author !== undefined && (typeof value.author !== "string" || !value.author.length || value.author.length > 120)) issue("author", "主题作者格式无效。");
  if (value.description !== undefined && (typeof value.description !== "string" || !value.description.length || value.description.length > 400)) issue("description", "主题描述格式无效。");
  if (!object(value.tokens) || !object(value.layout)) issue("tokens", "主题缺少有效的样式或布局。");
  else {
    const tokens = value.tokens; const layout = value.layout;
    keys(tokens, ["colors", "typography", "spacing", "radius"], [], "tokens");
    keys(layout, ["preset", "density", "quotaVisualization", "regionOrder", "accountSlots"], [], "layout");
    if (!object(tokens.colors)) issue("tokens.colors", "主题颜色格式无效。");
    else {
      keys(tokens.colors, Object.keys(defaultTheme.tokens.colors), [], "tokens.colors");
      for (const [key, color] of Object.entries(tokens.colors)) if (typeof color !== "string" || !/^#[a-f0-9]{6}$/i.test(color)) issue(`tokens.colors.${key}`, "颜色应采用 #RRGGBB 格式。");
      if (!issues.length) {
        const colors = tokens.colors as ThemeDocument["tokens"]["colors"];
        for (const foreground of ["text", "textMuted", "success", "warning", "exhausted", "error", "unknown", "stale", "accent"] as const) for (const background of ["background", "surface"] as const) if (contrast(colors[foreground], colors[background]) < 4.5) issue(`tokens.colors.${foreground}`, "主题文字对比度不足，请加深或调亮文字颜色。");
        if (contrast(colors.accentText, colors.accent) < 4.5) issue("tokens.colors.accentText", "强调文字对比度不足。");
        if (contrast(colors.border, colors.surface) < 3) issue("tokens.colors.border", "主题边界对比度不足。");
      }
    }
    if (!object(tokens.typography)) issue("tokens.typography", "主题字体格式无效。");
    else {
      keys(tokens.typography, ["fontFamily", "fontSize", "lineHeight"], [], "tokens.typography");
      if (!["system", "system-monospace"].includes(String(tokens.typography.fontFamily))) issue("tokens.typography.fontFamily", "主题仅支持系统字体。");
      if (!Number.isInteger(tokens.typography.fontSize) || Number(tokens.typography.fontSize) < 12 || Number(tokens.typography.fontSize) > 20) issue("tokens.typography.fontSize", "主题字号应为 12–20。");
      if (typeof tokens.typography.lineHeight !== "number" || tokens.typography.lineHeight < 1.2 || tokens.typography.lineHeight > 1.8) issue("tokens.typography.lineHeight", "主题行高应为 1.2–1.8。");
    }
    if (!object(tokens.spacing)) issue("tokens.spacing", "主题间距格式无效。");
    else {
      keys(tokens.spacing, ["padding", "gap"], [], "tokens.spacing");
      if (!Number.isInteger(tokens.spacing.padding) || Number(tokens.spacing.padding) < 4 || Number(tokens.spacing.padding) > 24 || !Number.isInteger(tokens.spacing.gap) || Number(tokens.spacing.gap) < 4 || Number(tokens.spacing.gap) > 20) issue("tokens.spacing", "主题间距超出范围。");
    }
    if (!Number.isInteger(tokens.radius) || Number(tokens.radius) < 0 || Number(tokens.radius) > 20) issue("tokens.radius", "主题圆角应为 0–20。");
    if (!["compact-rows", "cards", "horizontal"].includes(String(layout.preset)) || !["compact", "comfortable"].includes(String(layout.density)) || !["text", "bar-text"].includes(String(layout.quotaVisualization))) issue("layout", "主题布局不受支持。");
    for (const [key, expected] of [["regionOrder", ["accounts", "recommendation"]], ["accountSlots", ["account-label", "quota-windows", "availability"]]] as const) if (!Array.isArray(layout[key]) || layout[key].length !== expected.length || new Set(layout[key]).size !== expected.length || layout[key].some(item => !expected.includes(item as never))) issue(`layout.${key}`, "主题布局槽位需要完整且无重复。");
  }
  return { valid: !issues.length, issues };
}

function contrast(a: string, b: string): number {
  const luminance = (color: string) => [1, 3, 5].map(index => Number.parseInt(color.slice(index, index + 2), 16) / 255).map(v => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4).reduce((sum, v, index) => sum + v * [0.2126, 0.7152, 0.0722][index], 0);
  const x = luminance(a); const y = luminance(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
