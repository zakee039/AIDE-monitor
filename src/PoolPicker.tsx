import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { poolOptions } from "./quota-pools";
import { t } from "./i18n";

export function PoolPicker({ provider, name, value, disabled, onChange }: {
  provider: string; name: string; value: string[]; disabled: boolean; onChange: (value: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const options = poolOptions(provider);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  if (!options.length) return <span className="pool-unavailable">—</span>;
  const label = options.filter(p => value.includes(p.id)).map(p => t(p.label)).join("、") || t("选择额度池");
  return <div className="pool-picker" ref={root} onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setOpen(false); }} onKeyDown={e => { if (e.key === "Escape") { setOpen(false); root.current?.querySelector("button")?.focus(); e.stopPropagation(); } }}>
    <button type="button" className="pool-trigger" aria-label={`${t("额度池")} ${name}`} aria-expanded={open} disabled={disabled} title={label} onClick={() => setOpen(!open)}><span>{label}</span><ChevronDown size={13} /></button>
    {open && <div className="pool-menu" role="group" aria-label={`${t("额度池")} ${name}`}>
      {options.map(pool => <label key={pool.id}><input type="checkbox" checked={value.includes(pool.id)} disabled={disabled} onChange={e => onChange(e.target.checked ? options.filter(p => value.includes(p.id) || p.id === pool.id).map(p => p.id) : value.filter(id => id !== pool.id))} /><span className="selection-check"><Check size={12} /></span><span>{t(pool.label)}</span></label>)}
      <small>{t(provider === "grok" ? "产品消耗占比不作为独立额度池。" : "每个额度池单独展示；未返回的额度显示 —。")}</small>
    </div>}
  </div>;
}
