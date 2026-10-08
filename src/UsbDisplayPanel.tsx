import { UsbScreen, usbThemes } from "./UsbScreen";
import { FitPreview, ThemeStudio } from "./ThemeStudio";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Settings, Snapshot, UsbDisplaySettings, UsbThemeDefinition } from "../contracts/hud-api";
import { desktop } from "./api";
import { t } from "./i18n";
interface Display { id: string; name: string; width: number; height: number }
function parseTheme(value: unknown): UsbThemeDefinition {
  const theme = value as UsbThemeDefinition;
  const keys = ["background", "text", "textMuted", "border", "success"];
  if (!theme || theme.kind !== "aide-usb-theme" || theme.version !== 1 || theme.width !== 320 || theme.height !== 170 || !/^usb-custom-[a-z0-9-]+$/.test(theme.id) || theme.id.length > 80 || typeof theme.name !== "string" || !theme.name.trim() || [...theme.name].length > 40 || /[\x00-\x1f\x7f]/.test(theme.name) || !["rows", "columns"].includes(theme.layout) || !theme.palette || Object.keys(theme.palette).length !== 5 || keys.some(key => !/^#[0-9a-f]{6}$/i.test((theme.palette as Record<string,string>)[key]))) throw new Error(t("请选择有效的独立监视屏专用主题文件"));
  return { kind: theme.kind, version: theme.version, width: theme.width, height: theme.height, id: theme.id, name: theme.name, layout: theme.layout, palette: theme.palette };
}
export function UsbDisplayPanel({settings,snapshot,busy,onChange}:{settings:Settings;snapshot:Snapshot|null;busy:boolean;onChange:(value:UsbDisplaySettings)=>void}) {
  const [displays,setDisplays]=useState<Display[]>([]);
  const [error,setError]=useState("");
  const usb=settings.usbDisplay;
  const [selectedId,setSelectedId]=useState(usb.themeId);
  const input=useRef<HTMLInputElement>(null);
  const [now,setNow]=useState(Date.now());
  useEffect(()=>setSelectedId(usb.themeId),[usb.themeId]);
  useEffect(()=>{const timer=window.setInterval(()=>setNow(Date.now()),1000);return()=>window.clearInterval(timer);},[]);
  useEffect(()=>{let active=true;const scan=async()=>{if(!desktop)return;try{const value=await invoke<Display[]>("aide_usb_displays");if(active)setDisplays(value);}catch(e){if(active)setError(String(e));}};void scan();const timer=window.setInterval(scan,2000);return()=>{active=false;window.clearInterval(timer);};},[]);
  const connected=displays.some(d=>d.id===usb.deviceId);
  const themes=[...usbThemes,...(usb.customThemes??[])];
  const importFile=async(file:File)=>{try{setError("");if(file.size>32768)throw new Error(t("主题文件不能超过 32 KB"));const theme=parseTheme(JSON.parse(await file.text()));if((usb.customThemes??[]).some(item=>item.id===theme.id))throw new Error(t("主题 ID 已存在，请使用新的 ID"));const customThemes=[...(usb.customThemes??[]).filter(item=>item.id!==theme.id),theme];if(customThemes.length>20)throw new Error(t("最多导入 20 个 独立监视屏主题"));onChange({...usb,customThemes});setSelectedId(theme.id);}catch(e){setError(e instanceof Error?e.message:String(e));}finally{if(input.current)input.current.value="";}};
  return <section className="usb-panel"><h1>{t("独立监视屏")}</h1><p>{t("为小屏选择喜欢的样式，独立于桌面主题。")}</p>
    {error&&<p className="settings-alert" role="alert">{error}</p>}
    <input hidden type="file" ref={input} accept=".json" aria-label={t("导入独立监视屏主题")} onChange={event=>{const file=event.target.files?.[0];if(file)void importFile(file);}}/>
    <div className="settings-card usb-device-card">
      <div className="setting-row"><strong>{t("启用屏幕展示")}</strong><input type="checkbox" aria-label={t("启用屏幕展示")} checked={usb.enabled} disabled={busy||!usb.deviceId} onChange={e=>onChange({...usb,enabled:e.target.checked})}/></div>
      <div className="setting-row"><strong>{t("目标屏幕")}</strong><select aria-label={t("目标屏幕")} value={usb.deviceId} disabled={busy} onChange={e=>onChange({...usb,deviceId:e.target.value})}><option value="" disabled>{t("选择屏幕")}</option>{usb.deviceId&&!connected&&<option value={usb.deviceId}>{t("已记忆的屏幕（未连接）")}</option>}{displays.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select></div>
    </div>
    <ThemeStudio themes={themes} selectedId={selectedId} activeId={usb.themeId} onSelect={setSelectedId} onImport={()=>input.current?.click()} onApply={()=>onChange({...usb,themeId:selectedId})} busy={busy} detail="">
      <FitPreview><UsbScreen snapshot={snapshot} themeId={selectedId} customThemes={usb.customThemes} now={now}/></FitPreview>
    </ThemeStudio>

  </section>;
}
