import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { poolOptions } from "./quota-pools";
import { t } from "./i18n";

export function PoolPicker({ provider, name, value, disabled, onChange, pools, aliases, onAliasChange }: {
  pools?: { id: string; label: string }[]; provider: string; name: string; value: string[]; disabled: boolean; onChange: (value: string[]) => void;
  aliases: Record<string, string>; onAliasChange: (pool: string, name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState({ above: false, height: 350 });
  const root = useRef<HTMLDivElement>(null);
  const options = pools ?? poolOptions(provider);
  useLayoutEffect(() => {
    if (!open || !root.current) return;
    const bounds = root.current.closest('.settings-content')?.getBoundingClientRect();
    const rect = root.current.getBoundingClientRect();
    const below = (bounds?.bottom ?? window.innerHeight) - rect.bottom - 12;
    const above = rect.top - (bounds?.top ?? 0) - 12;
    const expected = Math.min(350, options.length * 60 + 80);
    const upwards = below < expected && above > below;
    setPlacement({ above: upwards, height: Math.max(80, Math.min(350, upwards ? above : below)) });
  }, [open, options.length]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  if (!options.length) return <span className="pool-unavailable">—</span>;
  const label = options.filter(p => value.includes(p.id)).map(p => aliases[p.id]?.trim() || t(p.label)).join("、") || t("选择额度池");
  return <div className="pool-picker" ref={root} onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setOpen(false); }} onKeyDown={e => { if (e.key === "Escape") { setOpen(false); root.current?.querySelector("button")?.focus(); e.stopPropagation(); } }}>
    <button type="button" className="pool-trigger" aria-label={`${t("额度池")} ${name}`} aria-expanded={open} disabled={disabled} title={label} onClick={() => setOpen(!open)}><span>{label}</span><ChevronDown size={13} /></button>
    {open && <div className={`pool-menu${placement.above ? " above" : ""}`} style={{ maxHeight: placement.height }} role="group" aria-label={`${t("额度池")} ${name}`}>
      {options.map(pool => <div className="pool-option" key={pool.id}>
        <label className="pool-check"><input type="checkbox" aria-label={`${t("选择额度池")} ${pool.label}`} checked={value.includes(pool.id)} disabled={disabled} onChange={e => onChange(e.target.checked ? options.filter(p => value.includes(p.id) || p.id === pool.id).map(p => p.id) : value.filter(id => id !== pool.id))} /><span className="selection-check"><Check size={12} /></span></label>
        <div className="pool-name"><input aria-label={`${t("显示名称")} ${pool.label}`} maxLength={80} value={aliases[pool.id] ?? ""} placeholder={t(pool.label)} disabled={disabled} onChange={e => onAliasChange(pool.id, e.target.value)} /><small className="pool-original" translate="no">{pool.label}</small></div>
      </div>)}
      <small>{t(provider === "grok" ? "产品消耗占比不作为独立额度池。" : "每个额度池单独展示；刷新失败时保留历史值并淡化。")}</small>
    </div>}
  </div>;
}
