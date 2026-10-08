import { useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { desktop } from './api';
import { t } from './i18n';
import { FitPreview, ThemeStudio } from './ThemeStudio';
interface Theme { id:string; name:string; version:string; author?:string; builtIn:boolean }
interface Catalog {themes:Theme[];activeId:string;notice:string|null}
async function request<T>(method:string,args:Record<string,unknown>={}):Promise<T>{const r=await invoke<{ok:boolean;data:T;error?:{message:string}}>(`aide_theme_${method}`,args);if(!r.ok)throw new Error(r.error?.message??'Theme operation failed');return r.data;}
export function ThemeManager({onChanged,renderPreview}:{onChanged:()=>Promise<void>;renderPreview:(id:string)=>ReactNode}){
  const [catalog,setCatalog]=useState<Catalog>({themes:[],activeId:'default',notice:null});
  const [selectedId,setSelectedId]=useState('default');
  const [demoActive,setDemoActive]=useState('default');
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [notice,setNotice]=useState('');const [clearStorage,setClearStorage]=useState(true);
  const reload=async()=>{if(desktop){const value=await request<Catalog>('list');value.themes=value.themes.map(theme=>theme.id==='paper'?{...theme,name:'白昼'}:theme);setCatalog(value);return value;}const value={activeId:demoActive,notice:null,themes:[{id:'default',name:'薄荷初音',version:'内置',builtIn:true},{id:'midnight',name:'黑暗',version:'内置',builtIn:true},{id:'paper',name:'白昼',version:'内置',builtIn:true}]};setCatalog(value);return value;};
  useEffect(()=>{let active=true;void reload().then(value=>{if(active)setSelectedId(value.activeId);}).catch(e=>setError(e.message));let stop:(()=>void)|undefined;if(desktop)void listen('aide://themes-changed',()=>{setNotice('');void reload().catch(e=>setError(e.message));void onChanged();}).then(fn=>{if(active)stop=fn;else fn();});return()=>{active=false;stop?.();};},[]);
  const run=async(method:string,args:Record<string,unknown>={})=>{setBusy(true);setError('');setNotice('');try{
    if(!desktop){if(method==='select'){setDemoActive(selectedId);setCatalog(value=>({...value,activeId:selectedId}));}else setNotice(t('请在桌面应用中导入主题'));return;}
    const result=await request<{loading?:boolean;cancelled?:boolean}>(method,args);const value=await reload();await onChanged();
    if(method==='uninstall')setSelectedId(value.activeId);
    if(method==='install'&&!result?.cancelled){const added=value.themes.find(theme=>!catalog.themes.some(old=>old.id===theme.id));if(added)setSelectedId(added.id);}
    if(result?.loading)setNotice(t('正在加载主题，失败时会自动恢复。'));
  }catch(e){setError(e instanceof Error?e.message:String(e));}finally{setBusy(false);}};
  const selected=catalog.themes.find(theme=>theme.id===selectedId);
  return <section className="theme-manager"><h1>{t('外观')}</h1><p>{t('为桌面小窗选择喜欢的样式。')}</p>{(error||catalog.notice||notice)&&<p className="settings-alert" role="status">{error||catalog.notice||notice}</p>}
    <ThemeStudio themes={catalog.themes} selectedId={selectedId} activeId={catalog.activeId} onSelect={setSelectedId} onImport={()=>void run('install')} onApply={()=>void run('select',{id:selectedId})} busy={busy} detail={t('桌面悬浮窗 · 点击应用后生效')}>
      <FitPreview>{selected?.builtIn ? renderPreview(selectedId) : <div className="studio-widget-placeholder"><strong>{selected?.name}</strong>{t('独立 Widget 主题，应用后在桌面窗口展示。')}</div>}</FitPreview>
    </ThemeStudio>
    {selected&&!selected.builtIn&&<div className="studio-theme-management"><label><input type="checkbox" checked={clearStorage} onChange={e=>setClearStorage(e.target.checked)}/>{t('卸载时清理主题偏好')}</label><button className="text-button" disabled={busy} onClick={()=>void run('uninstall',{id:selected.id,clearStorage})}>{t('卸载')}</button></div>}
  </section>;
}
