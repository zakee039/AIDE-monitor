/** V1 public contract. v0.1.0 implements Tauri IPC; HTTP/SSE remain planned. */
export type UtcTime = string; // RFC 3339 UTC, validated at every transport boundary
export type AccountId = string; // HUD-generated opaque ID, never an email/token
export type Scope =
  | "quota.read" | "quota.refresh" | "events.read"
  | "settings.read" | "settings.write" | "themes.read" | "themes.write"
  | "window.control" | "diagnostics.read";

export type ErrorCode =
  | "INVALID_ARGUMENT" | "UNAUTHORIZED" | "FORBIDDEN" | "NOT_FOUND"
  | "VERSION_UNSUPPORTED" | "CONFLICT" | "RATE_LIMITED"
  | "SOURCE_NOT_FOUND" | "SOURCE_BUSY" | "SOURCE_UNSUPPORTED"
  | "STORAGE_KEY_UNAVAILABLE" | "STORAGE_DECRYPT_FAILED"
  | "ACCOUNT_UNSUPPORTED" | "AUTH_EXPIRED" | "UPSTREAM_FORBIDDEN"
  | "NETWORK_ERROR" | "TIMEOUT" | "RESPONSE_UNSUPPORTED"
  | "THEME_INVALID" | "IO_ERROR" | "INTERNAL_ERROR";

export interface ApiError {
  code: ErrorCode;
  message: string; // safe display text, never raw upstream/file errors
  retryAt: UtcTime | null;
  retryable: boolean;
}

export interface EnvelopeMeta {
  apiVersion: "1.0";
  instanceId: string; // new UUID each process start
  revision: number; // monotonic safe integer within instance; never epoch time
  requestId: string;
  generatedAt: UtcTime;
}
export type ApiResult<T> = EnvelopeMeta & (
  | { ok: true; data: T }
  | { ok: false; error: ApiError }
);

export interface AccountSummary {
  alias?: string | null;
  isCurrent?: boolean;
  id: AccountId;
  displayName: string;
  providerId: string;
  selected: boolean;
  order: number;
  support: "supported" | "unsupported" | "unknown";
}

export interface QuotaWindow {
  id: string; // unique and stable within an account
  scope: "base" | `feature:${string}`;
  kind: "primary" | "secondary" | "other";
  label: string;
  durationSeconds: number | null;
  applicability: "required" | "not_applicable" | "unknown";
  measurement: "percent" | "unlimited" | "unknown";
  remainingPercent: number | null; // 0..100, unrounded
  exhausted: boolean | null;
  resetsAt: UtcTime | null;
}

export interface AccountQuota {
  resetCreditsAvailable?: number | null; // Available manual resets; absent/invalid stays unknown.
  accountId: AccountId;
  origin: "network" | "cockpit_cache" | "none";
  freshness: "fresh" | "stale" | "unknown";
  status: "ok" | "refreshing" | "auth_expired" | "unavailable" | "error";
  lastSuccessAt: UtcTime | null;
  lastAttemptAt: UtcTime | null;
  observedAt: UtcTime | null; // cache origin may have its own sample time
  validUntil: UtcTime | null; // age deadline; reset boundaries may invalidate earlier
  providerAllowed: boolean | null;
  baseCoverageComplete: boolean;
  blockingReason: "none" | "quota_windows" | "other" | "unknown";
  windows: QuotaWindow[];
  error: ApiError | null; // last completed failure; retained during retry until success
}

export type AvailabilityReason =
  | "available" | "below_threshold" | "quota_exhausted" | "no_data" | "stale"
  | "awaiting_confirmation" | "incomplete_quota" | "other_limit"
  | "unknown_reset" | "query_failed" | "cache_only" | "clock_uncertain";
export interface AccountAvailability {
  accountId: AccountId;
  state: "now" | "waiting" | "unknown";
  reason: AvailabilityReason;
  estimatedAvailableAt: UtcTime | null;
}
export interface Recommendation {
  state: "now" | "waiting" | "unknown" | "empty";
  accountId: AccountId | null;
  estimatedAvailableAt: UtcTime | null;
  reason: string; // stable reason code, UI localizes it
  coverage: { selected: number; known: number; unknown: number; complete: boolean };
}
export interface SourceStatus {
  state: "ready" | "not_configured" | "unavailable" | "unsupported";
  format: "plaintext" | "encrypted_v1" | "mixed" | "unknown";
  adapterVersion: string;
  error: ApiError | null;
}
export interface Snapshot {
  source: SourceStatus;
  accounts: AccountSummary[]; // selected accounts only
  quotas: AccountQuota[];
  availability: AccountAvailability[];
  recommendation: Recommendation;
  totalQuota?: { percent: number | null; partial: boolean; weeklyScalePercent: number }; // Estimate; 15% is a local heuristic.
  nextRefreshAt: UtcTime | null;
}

export interface RefreshRequest { accountIds?: AccountId[] } // omitted = all selected
export interface RefreshTicket { jobId: string; joined: boolean }
export interface RefreshJob {
  jobId: string;
  state: "queued" | "running" | "completed" | "partial" | "failed" | "cancelled";
  createdAt: UtcTime;
  finishedAt: UtcTime | null;
  items: Array<{
    accountId: AccountId;
    state: "queued" | "running" | "succeeded" | "failed" | "cancelled";
    error: ApiError | null;
  }>;
}

export interface DisplaySettings {
  positionLocked: boolean;
  alwaysOnTop: boolean;
  showHoverDetails: boolean;
  privacyMode: boolean;
  locale: "system" | "zh-CN" | "en";
}
export interface UsbDisplaySettings { enabled: boolean; deviceId: string; themeId: string }
export interface ProxyProfile { id: string; name: string; address: string }
export interface Settings {
  proxies: ProxyProfile[];
  accountProxies: Record<string, string>;
  accountRefresh: Record<string, number>;
  usbDisplay: UsbDisplaySettings;
  settingsRevision: number;
  refreshIntervalSeconds: number;
  autoRefresh: boolean;
  display: DisplaySettings;
  activeThemeId: string;
}
export interface SettingsPatch {
  proxies?: ProxyProfile[];
  accountProxies?: Record<string, string>;
  accountRefresh?: Record<string, number>;
  usbDisplay?: UsbDisplaySettings;
  expectedRevision: number;
  refreshIntervalSeconds?: number;
  autoRefresh?: boolean;
  display?: Partial<DisplaySettings>;
}
export interface AccountSelectionPatch {
  expectedRevision: number;
  accountIds: AccountId[]; // unique ordered selection; full replacement
  aliases?: Record<AccountId, string>; // local presentation only
}

export interface ThemeSummary { id: string; name: string; builtIn: boolean }
export interface ThemeValidation {
  valid: boolean;
  issues: Array<{ path: string; code: string; message: string }>;
}
export interface Diagnostics {
  appVersion: string;
  adapterVersion: string;
  source: SourceStatus;
  selectedAccountCount: number;
  activeJobCount: number;
  recentErrorCodes: ErrorCode[];
}
export interface Capabilities {
  appVersion: string;
  apiVersion: "1.0";
  transport: "tauri" | "http";
  enabledMethods: string[];
  grantedScopes: Scope[];
  themeSchemaVersions: number[];
  providerIds: string[];
  maxRefreshAccounts: number;
}

/** Logical methods, not literal Tauri command names or HTTP routes. */
export interface MethodMap {
  "capabilities.get": { params: Record<string, never>; result: Capabilities };
  "accounts.alias.update": { params: { accountId: string; alias: string }; result: Settings };
  "accounts.list": { params: Record<string, never>; result: AccountSummary[] };
  "accounts.selection.update": { params: AccountSelectionPatch; result: Settings };
  "quota.snapshot.get": { params: Record<string, never>; result: Snapshot };
  "recommendation.get": { params: Record<string, never>; result: Recommendation };
  "refresh.request": { params: RefreshRequest; result: RefreshTicket };
  "refresh.status.get": { params: { jobId: string }; result: RefreshJob };
  "settings.get": { params: Record<string, never>; result: Settings };
  "settings.update": { params: SettingsPatch; result: Settings };
  "themes.list": { params: Record<string, never>; result: ThemeSummary[] };
  "window.control": { params: { action: "show" | "hide" | "restore_position" | "open_settings" }; result: { accepted: boolean } };
  "diagnostics.get": { params: Record<string, never>; result: Diagnostics };
}

export type EventName =
  | "snapshot.changed" | "refresh.progress" | "refresh.completed"
  | "source.changed" | "settings.changed" | "theme.changed" | "resync.required";
export interface HudEvent {
  apiVersion: "1.0";
  instanceId: string;
  revision: number;
  sequence: number;
  emittedAt: UtcTime;
  type: EventName;
  data: { jobId?: string }; // invalidation notifications; authoritative data via get
}

/** SDK shape only. A real implementation must validate JSON and enforce capabilities. */
export interface HudClient {
  call<M extends keyof MethodMap>(
    method: M,
    params: MethodMap[M]["params"],
  ): Promise<ApiResult<MethodMap[M]["result"]>>;
  subscribe(listener: (event: HudEvent) => void): Promise<() => void>;
}
