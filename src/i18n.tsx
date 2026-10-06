import { cloneElement, isValidElement, type ReactNode } from "react";

let language = "en";
export function setLanguage(locale: string) { language = locale; }
const en: Record<string, string> = {
  "保留三套内置主题，也可安装独立的 Widget 界面。":"Choose a built-in theme or install a custom widget.",
  "正在加载主题，失败时会自动恢复。":"Loading theme. The previous interface will recover if loading fails.",
  "重新加载":"Reload", "卸载":"Uninstall", "安装 Widget 主题":"Install a widget theme",
  "选择 .aidetheme 包，所有资源须包含在包内。":"Choose an .aidetheme package containing all of its assets.",
  "安装主题":"Install theme", "卸载时清理主题偏好":"Clear theme preferences when uninstalling",
  "主题通过受限 API 读取已选择账号，不允许任意联网。加载失败可从托盘恢复内置主题。":"Themes can read selected accounts through a restricted API. External network access is blocked. Restore a built-in theme from the tray if needed.",

  "gemini额度": "Gemini quota",
  "开机自启":"Launch at startup", "选择源":"Select sources", "选择数据源":"Data sources", "官方客户端":"Official client", "未检测到目录":"No folder detected", "本地账号导出 JSON":"Local account export JSON", "OAuth 登录账号":"OAuth sign-in accounts", "保存并扫描":"Save & scan", "数据源已保存并扫描。":"Sources saved and scanned.",
  "折叠悬浮窗": "Collapse HUD", "展开悬浮窗": "Expand HUD", "估算总额度": "Estimated total quota", "部分账号数据不可用": "Some account data is unavailable",
  "设置分区": "Settings sections", "浏览器演示 · 虚构账号": "Browser demo · Sample accounts",
  "常规": "General", "账号": "Accounts", "外观": "Appearance", "关于": "About", "/ 设置": "/ Settings", "设置": "Settings",
  "语言": "Language", "界面语言": "Interface language", "更改后立即保存": "Changes save automatically",
  "连接你的账号，调整刷新与窗口行为。": "Connect your accounts and adjust refresh and window behavior.",
  "数据源": "Account source", "读取官方客户端或 Cockpit Tools 登录账号": "Use official client or Cockpit Tools sign-ins",
  "默认优先读取 CODEX_HOME/auth.json（通常为 ~/.codex/auth.json）。系统凭据库或仅内存登录暂不支持。": "Automatically checks CODEX_HOME/auth.json (usually ~/.codex/auth.json). OS credential stores and memory-only sign-ins are not supported yet.",
  "只读 auth.json 或 Cockpit 账号；请在原客户端登录。": "Read-only access. Sign in through the original client.",
  "选择目录": "Choose folder", "重新扫描": "Rescan", "数据源已连接": "Source connected", "尚未连接数据源": "No source connected", "数据源暂不可用": "Source unavailable", "数据源格式不受支持": "Unsupported source format", "未连接": "Not connected",
  "选择官方账号或 Cockpit 数据目录": "Choose an official or Cockpit account folder",
  "配额刷新": "Quota refresh", "自动刷新": "Auto refresh", "定期获取所选账号的最新配额": "Keep selected account quotas up to date", "刷新间隔": "Refresh interval", "手动刷新与自动刷新共用调度器": "Manual and automatic refresh share one scheduler",
  "窗口与显示": "Window & display", "始终置顶": "Always on top", "让 HUD 保持在其他窗口上方": "Keep the HUD above other windows", "隐私模式": "Privacy mode", "用“账号 1”等名称遮罩账号显示": "Replace account names with numbered labels",
  "凭据只留在本机": "Credentials stay local", "读取凭据与配额均由本机后台处理": "Credentials and quota queries are handled locally", "同步状态": "Sync status",
  "勾选要显示的账号，并调整 HUD 中的顺序。": "Choose accounts to display and arrange their HUD order.",
  " 个已选择 / ": " selected / ", " 个账号": " accounts", "最多显示 ": "Maximum: ", "保存选择": "Save selection", "显示名称": "Display name", "正在保存…": "Saving…", "当前本机登录": "Local sign-in", "已发现": "Found", "暂不支持": "Unsupported", "支持情况待核实": "Support unverified",
  "尚未发现账号": "No accounts found", "选择含 auth.json 或 codex_accounts.json 的目录，再重新扫描。": "Choose a folder containing auth.json or codex_accounts.json, then rescan.", "选择数据目录": "Choose account folder", "账号选择已保存。": "Account selection saved.", "账号列表已重新读取。": "Accounts rescanned.",
  "选择内置主题，或导入自己编写的主题文件。": "Choose a built-in theme or import your own theme file.", "奶油薄荷": "Cream Mint", "午夜蓝": "Midnight", "晨光": "Daylight", "纸白": "Daylight", "内置主题": "Built-in theme", "自定义主题": "Custom theme", "应用": "Apply", "添加自己的主题": "Add your own theme", "用 theme.json 定制颜色、字体与布局": "Customize colors, typography and layout with theme.json", "导入主题": "Import theme", "下载示例": "Download example", "主题仅接受本地 JSON，导入前会校验格式与可读性。": "Local JSON themes are validated before import.", "主题只改变外观，不改变配额判断": "Themes change appearance, not quota rules", "应用此主题": "Apply theme", "关闭主题预览": "Close theme preview", " · 虚构数据": " · Sample data", "主题已应用。": "Theme applied.", "主题示例已保存。": "Theme example saved.",
  "专注配额，轻量常驻。": "Your quotas, at a glance.", "Tauri 2 + Rust · 开放接口 · 可自行扩展主题": "Tauri 2 + Rust · Open interfaces · Custom themes",
  "读取官方客户端或 Cockpit Tools 登录账号，展示配额与重置时间。认证失效请在原客户端重新登录。": "Show quota and reset times for official client or Cockpit Tools sign-ins. Renew expired authentication in the original client.",
  "正在桌面应用中运行": "Running in the desktop app", "浏览器演示，全部数据均为虚构": "Browser demo — all data is fictional", "开放能力": "Open capabilities", "以当前应用实际提供的方法为准": "Methods supported by this application", "接口说明": "API reference", "可用方法": "Methods", "接口版本": "API version", "主题版本": "Theme schema", "第一版通过应用内部接口调用。": "Available through the in-app interface.", "检查状态": "Check status", "适配器版本": "Adapter version", "所选账号 / 进行中的刷新": "Selected accounts / active refreshes", "最近错误": "Recent errors", "暂无": "None", "恢复悬浮窗位置": "Restore HUD position", "窗口位置已恢复。": "HUD position restored.",
  "刷新全部": "Refresh all", "隐藏悬浮窗": "Hide HUD", "账号配额": "Account quotas", "尚未选择账号": "No accounts selected", "尚未连接账号来源": "No account source connected", "右键托盘 → 设置": "Right-click tray → Settings", "正在读取配额…": "Loading quotas…", "正在读取设置": "Loading settings", "读取失败 · 点击重试": "Could not load — retry", "返回 HUD": "Back to HUD", "预览设置": "Preview settings", "浏览器预览 · 全部账号与配额均为虚构": "Browser preview · All accounts and quotas are fictional", "虚构数据预览 · 更多操作位于托盘右键菜单": "Sample data · More actions in the tray menu",
  "不适用": "N/A", "无限": "Unlimited", "未知": "Unknown", "时间未知": "Time unknown", "待核实": "Unverified", "认证过期": "Sign-in expired", "查询失败": "Query failed", "过期 · 更新中": "Stale · Refreshing", "已过期": "Stale", "更新中": "Refreshing", "可用": "Available", "等待重置": "Waiting for reset", "暂无配额": "No quota data", "数据已过期": "Stale data", "等待核实": "Awaiting verification", "配额不完整": "Incomplete quota", "其他限制": "Other limits", "重置时间未知": "Reset time unknown", "仅缓存数据": "Cached data only", "时间需核实": "Clock unverified", "低于推荐额度": "Below recommended quota", "额度已耗尽": "Quota exhausted", "当前可用": "Available now", "待查询": "Pending query", "重置次数未知": "Reset count unknown", " 秒": " seconds", "暂时无法完成操作，请重试。": "Unable to complete the action. Please retry."
};

export function t(text: string): string {
  if (language === "zh-CN") return text;
  if (en[text] !== undefined) return en[text];
  const trimmed = text.trim();
  if (en[trimmed] !== undefined) return text.replace(trimmed, en[trimmed]);
  if (/^\d+ 分钟$/.test(text)) return text.replace(" 分钟", " min");
  for (const [prefix, translated] of [["刷新 ", "Refresh "], ["上移 ", "Move up "], ["下移 ", "Move down "], ["显示名称 ", "Display name "], ["预览 ", "Preview "], ["应用 ", "Apply "], ["可用重置次数 ", "Available resets "]]) {
    if (text.startsWith(prefix)) return translated + text.slice(prefix.length);
  }
  return text.replace(/ 已应用$/, " applied").replace(/账号 (\d+)/g, "Account $1").replace(" 剩余 ", " remaining ");
}

// Keep content and accessibility labels in sync; user-supplied names opt out.
export function localize(node: ReactNode): ReactNode {
  if (typeof node === "string") return t(node);
  if (Array.isArray(node)) return node.map(localize);
  if (!isValidElement<Record<string, unknown>>(node) || node.props.translate === "no") return node;
  const props: Record<string, unknown> = {};
  for (const key of ["title", "description", "detail", "aria-label"]) {
    if (typeof node.props[key] === "string") props[key] = t(node.props[key]);
  }
  if (node.props.children !== undefined) props.children = localize(node.props.children as ReactNode);
  return cloneElement(node, props);
}
