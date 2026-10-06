import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Settings, UsbDisplaySettings } from "../contracts/hud-api";
import { desktop } from "./api";
import { t } from "./i18n";
interface Display { id: string; name: string; width: number; height: number }
export function UsbDisplayPanel({settings,busy,onChange}:{settings:Settings;busy:boolean;onChange:(value:UsbDisplaySettings)=>void}) {
  const [displays,setDisplays]=useState<Display[]>([]);
  const [error,setError]=useState("");
  const usb=settings.usbDisplay;
  useEffect(()=>{let active=true;const scan=async()=>{if(!desktop)return;try{const value=await invoke<Display[]>("aide_usb_displays");if(active){setDisplays(value);setError("");}}catch(e){if(active)setError(String(e));}};void scan();const timer=window.setInterval(scan,2000);return()=>{active=false;window.clearInterval(timer);};},[]);
  const connected=displays.some(d=>d.id===usb.deviceId);
  return <section className="usb-panel"><h1>{t("USB 监视屏")}</h1>
    <p>{t("独立于悬浮窗，自动铺满选定屏幕。设备断开后等待原屏幕重新连接。")}</p>
    {error&&<p role="alert">{error}</p>}
    <div className="settings-card">
      <div className="setting-row"><strong>{t("启用屏幕展示")}</strong><input type="checkbox" aria-label={t("启用屏幕展示")} checked={usb.enabled} disabled={busy||!usb.deviceId} onChange={e=>onChange({...usb,enabled:e.target.checked})}/></div>
      <div className="setting-row"><strong>{t("目标屏幕")}</strong><select aria-label={t("目标屏幕")} value={usb.deviceId} disabled={busy} onChange={e=>onChange({...usb,deviceId:e.target.value})}><option value="" disabled>{t("选择屏幕")}</option>{usb.deviceId&&!connected&&<option value={usb.deviceId}>{t("已记忆的屏幕（未连接）")}</option>}{displays.map(d=><option key={d.id} value={d.id}>{d.name} · {d.width} × {d.height} · {d.id.slice(-16)}</option>)}</select></div>
      <div className="setting-row"><strong>{t("主题")}</strong><select aria-label={t("屏幕主题")} value={usb.themeId} disabled={busy} onChange={e=>onChange({...usb,themeId:e.target.value})}><option value="default">{t("薄荷初音")}</option><option value="midnight">{t("黑暗")}</option><option value="paper">{t("明亮")}</option></select></div>
      <p role="status">{t(!usb.enabled?"未启用":connected?"屏幕已连接":"等待原屏幕重新连接")}</p>
    </div><p>{t("自动检测 Windows 显示器；仅支持在系统显示设置中可见的 USB 小屏。")}</p>
  </section>;
}
