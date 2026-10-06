import { useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { desktop, errorMessage, internal, type UpdateInfo } from "./api";
import { t } from "./i18n";

export function UpdatePanel() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const check = async () => {
    setBusy(true); setError(""); setInfo(null);
    try { setInfo(await internal("update_check", {})); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  };
  return <div className="settings-card update-panel">
    <div className="proxy-heading"><h2><Download size={17} />{t("版本更新")}</h2><button className="secondary-button" disabled={busy || !desktop} onClick={() => void check()}><RefreshCw size={14} className={busy ? "spin" : ""} />{t(busy ? "正在检查…" : "检查更新")}</button></div>
    <p>{t("通过 GitHub Releases 检查最新正式版本。")}</p>
    {!desktop && <p>{t("请在桌面应用中检查更新。")}</p>}
    {error && <p role="alert">{error}</p>}
    {info && <div role="status"><p>{t(info.updateAvailable ? "发现新版本" : "当前已是最新版本")} · v{info.latestVersion}</p>{info.updateAvailable && <><button className="primary-button" onClick={() => { void internal("update_open", {}).catch(e => setError(errorMessage(e))); }}>{t("前往 GitHub 下载")}</button><details><summary>{t("版本说明")}</summary><pre>{info.notes}</pre></details></>}</div>}
  </div>;
}
