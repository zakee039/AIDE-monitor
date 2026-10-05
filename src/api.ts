import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ApiError, ApiResult, HudClient, HudEvent, MethodMap, SourceStatus } from "../contracts/hud-api";
import { demoCall, demoInternal, demoSubscribe } from "./demo";
import type { ThemeDocument } from "./themes";
import { t } from "./i18n";

export const desktop = isTauri();

export class HudApiError extends Error {
  constructor(readonly detail: ApiError) { super(detail.message); }
}

function envelope<T>(value: unknown): ApiResult<T> {
  if (!value || typeof value !== "object") throw new Error("invalid envelope");
  const result = value as ApiResult<T>;
  if (result.apiVersion !== "1.0" || typeof result.instanceId !== "string" || !Number.isSafeInteger(result.revision) || result.revision < 0 || typeof result.requestId !== "string" || typeof result.generatedAt !== "string" || !Number.isFinite(Date.parse(result.generatedAt))) throw new Error("invalid envelope metadata");
  if (result.ok === true && "data" in result) return result;
  if (result.ok === false && typeof result.error?.code === "string" && typeof result.error?.message === "string") return result;
  throw new Error("invalid envelope body");
}

export const hud: HudClient = {
  async call(method, params) {
    const value = desktop
      ? await invoke(`hud_v1_${method.replaceAll(".", "_")}`, { request: params })
      : await demoCall(method, params);
    return envelope(value);
  },
  async subscribe(listener) {
    if (!desktop) return demoSubscribe(listener);
    return listen<HudEvent>("hud://v1/event", ({ payload }) => {
      if (payload.apiVersion === "1.0" && typeof payload.instanceId === "string" && Number.isSafeInteger(payload.revision) && Number.isSafeInteger(payload.sequence) && typeof payload.type === "string") listener(payload);
    });
  },
};

export async function call<M extends keyof MethodMap>(method: M, params: MethodMap[M]["params"]): Promise<ApiResult<MethodMap[M]["result"]> & { ok: true }> {
  const result = await hud.call(method, params);
  if (!result.ok) throw new HudApiError(result.error);
  return result;
}

interface InternalMethods {
  window_layout: { params: { collapsed: boolean; width: number; height: number }; data: { accepted: boolean } };
  docs_open: { params: Record<string, never>; data: { opened: boolean } };
  theme_export: { params: Record<string, never>; data: { cancelled: boolean } };
  source_get: { params: Record<string, never>; data: { path: string | null } };
  source_choose: { params: Record<string, never>; data: { cancelled: boolean; path: string | null; source: SourceStatus } };
  source_rescan: { params: Record<string, never>; data: SourceStatus };
  theme_get: { params: { id?: string }; data: ThemeDocument };
}

export async function internal<M extends keyof InternalMethods>(method: M, request: InternalMethods[M]["params"]): Promise<InternalMethods[M]["data"]> {
  const result = envelope<InternalMethods[M]["data"]>(desktop ? await invoke(`hud_internal_${method}`, { request }) : await demoInternal(method, request));
  if (!result.ok) throw new HudApiError(result.error);
  return result.data;
}

export function errorMessage(error: unknown): string {
  if (!(error instanceof HudApiError)) return t("暂时无法完成操作，请重试。");
  if (document.documentElement.lang === "zh-CN") return error.message;
  const messages: Record<string, string> = {
    AUTH_EXPIRED: "Sign in again in the original client, then rescan.",
    CONFLICT: "Settings changed. Sync status and retry.",
    SOURCE_NOT_FOUND: "Account source not found. Choose an account folder.",
    SOURCE_UNSUPPORTED: "This account source format is not supported.",
    SOURCE_BUSY: "The account file is being updated. Retry shortly.",
    RATE_LIMITED: "Refresh is cooling down. Try again shortly.",
    INVALID_ARGUMENT: "Check the entered values and retry.",
    IO_ERROR: "Could not read or save the file. Check folder access.",
    NETWORK_ERROR: "Could not connect to the quota service.",
    FORBIDDEN: "This window cannot perform that action.",
    NOT_FOUND: "The requested item no longer exists. Sync status and retry.",
  };
  return `${messages[error.detail.code] ?? "Unable to complete the action. Please retry."} (${error.detail.code})`;
}
