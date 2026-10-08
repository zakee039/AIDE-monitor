import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, ChevronLeft, ChevronRight, Upload } from "lucide-react";
import { t } from "./i18n";
import "./theme-studio.css";

export function ThemeStudio({ themes, selectedId, activeId, onSelect, onImport, onApply, busy, children, detail }: {
  themes: { id: string; name: string }[]; selectedId: string; activeId: string;
  onSelect: (id: string) => void; onImport: () => void; onApply: () => void;
  busy: boolean; children: ReactNode; detail: string;
}) {
  const selected = themes.find(theme => theme.id === selectedId);
  const index = themes.findIndex(theme => theme.id === selectedId);
  const strip = useRef<HTMLDivElement>(null);
  useEffect(() => { strip.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" }); }, [selectedId]);
  const step = (direction: number) => onSelect(themes[(index + direction + themes.length) % themes.length].id);
  return <div className="theme-studio">
    <div className="studio-toolbar"><button className="studio-import" disabled={busy} onClick={onImport}><Upload size={15}/>{t("导入主题")}</button><button className="primary-button studio-apply" disabled={busy || !selected || selectedId === activeId} onClick={onApply}>{selectedId === activeId ? <><Check size={15}/>{t("已应用")}</> : t("应用")}</button></div>
    <div className="studio-selector"><button aria-label={t("上一个主题")} disabled={busy || themes.length < 2} onClick={()=>step(-1)}><ChevronLeft size={18}/></button><div className="studio-tabs" role="tablist" aria-label={t("主题")} ref={strip}>{themes.map(theme=><button role="tab" aria-selected={theme.id === selectedId} aria-controls="theme-preview-panel" key={theme.id} disabled={busy} onClick={()=>onSelect(theme.id)} onKeyDown={event=>{if(event.key === "ArrowRight" || event.key === "ArrowLeft"){event.preventDefault();step(event.key === "ArrowRight" ? 1 : -1);}}}><span>{t(theme.name)}</span>{theme.id === activeId && <i aria-label={t("已应用")}/>}</button>)}</div><button aria-label={t("下一个主题")} disabled={busy || themes.length < 2} onClick={()=>step(1)}><ChevronRight size={18}/></button></div>
    <div className="studio-preview" id="theme-preview-panel" role="tabpanel"><div className="studio-preview-heading"><strong>{t(selected?.name ?? "主题")}</strong><span>{t("预览")}</span></div>{children}{detail && <div className="studio-preview-caption">{detail}</div>}</div>
  </div>;
}

export function FitPreview({ children }: { children: ReactNode }) {
  const outer = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ scale: 1, width: 320, height: 170 });
  useEffect(() => {
    const measure = () => {
      if (!outer.current || !inner.current) return;
      const width = inner.current.offsetWidth, height = inner.current.offsetHeight;
      const scale = Math.min(1.25, (outer.current.clientWidth - 32) / Math.max(1, width));
      setSize({ scale, width: width * scale, height: height * scale });
    };
    const observer = new ResizeObserver(measure);
    if (outer.current) observer.observe(outer.current);
    if (inner.current) observer.observe(inner.current);
    measure(); return () => observer.disconnect();
  }, []);
  return <div className="studio-canvas" ref={outer}><div style={{ width: size.width, height: size.height }} className="studio-fit"><div className="studio-fit-content" ref={inner} style={{ transform: `scale(${size.scale})` }}>{children}</div></div></div>;
}
