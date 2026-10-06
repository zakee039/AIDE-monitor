import { useEffect, useRef, useState } from "react";
import { Network, Plus, X } from "lucide-react";
import type { ProxyProfile, Settings } from "../contracts/hud-api";
import { call, errorMessage, HudApiError } from "./api";
import { t } from "./i18n";

export function ProxySettings({ settings, reload }: { settings: Settings; reload: () => Promise<void> }) {
  const [profiles, setProfiles] = useState(settings.proxies);
  const [status, setStatus] = useState("");
  const draft = useRef(settings.proxies);
  const saved = useRef(JSON.stringify(settings.proxies));
  const running = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    if (!running.current && JSON.stringify(draft.current) === saved.current) {
      draft.current = settings.proxies;
      saved.current = JSON.stringify(settings.proxies);
      setProfiles(settings.proxies);
    }
  }, [settings.proxies]);
  const flush = async () => {
    if (running.current) return;
    running.current = true;
    if (mounted.current) setStatus(t("正在保存…"));
    try {
      while (JSON.stringify(draft.current) !== saved.current) {
        const pending = draft.current;
        for (let attempt = 0; ; attempt++) {
          const current = (await call("settings.get", {})).data;
          try {
            await call("settings.update", { expectedRevision: current.settingsRevision, proxies: pending });
            break;
          } catch (error) {
            if (!(error instanceof HudApiError) || error.detail.code !== "CONFLICT" || attempt >= 2) throw error;
          }
        }
        saved.current = JSON.stringify(pending);
      }
      if (mounted.current) setStatus(t("已自动保存"));
    } catch (error) {
      if (mounted.current) setStatus(errorMessage(error));
    } finally { running.current = false; }
    await reload();
  };
  const change = (next: ProxyProfile[]) => { draft.current = next; setProfiles(next); void flush(); };
  return <div className="settings-card proxy-settings">
    <div className="proxy-heading"><h2><Network size={17} />{t("代理地址设置")}</h2><button className="secondary-button" disabled={profiles.length >= 100} onClick={() => change([...draft.current, { id: crypto.randomUUID(), name: "", address: "" }])}><Plus size={14} />{t("添加代理")}</button></div>
    <p>{t("每行一组，输入即保存。在账号页选择代理，默认使用系统设置。")}</p>
    {profiles.map((profile, index) => <div className="proxy-row" key={profile.id}>
      <input aria-label={`${t("代理名称")} ${index + 1}`} placeholder={t("名称，如腾讯云")} maxLength={100} value={profile.name} onChange={e => change(draft.current.map(p => p.id === profile.id ? { ...p, name: e.target.value } : p))} />
      <input aria-label={`${t("代理地址")} ${index + 1}`} placeholder="socks5://127.0.0.1:7893" maxLength={2048} spellCheck={false} value={profile.address} onChange={e => change(draft.current.map(p => p.id === profile.id ? { ...p, address: e.target.value } : p))} />
      <button className="icon-button" aria-label={`${t("删除代理")} ${index + 1}`} onClick={() => change(draft.current.filter(p => p.id !== profile.id))}><X size={16} /></button>
    </div>)}
    <p role="status">{status || t("支持 HTTP、HTTPS、SOCKS5。删除代理后，关联账号恢复为系统设置。")}</p>
  </div>;
}
