import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { desktop } from './api';
import { t } from './i18n';
interface Theme { id:string; name:string; version:string; author?:string; builtIn:boolean }
interface Catalog {themes:Theme[];activeId:string;notice:string|null}
async function request<T>(method:string,args:Record<string,unknown>={}):Promise<T>{const r=await invoke<{ok:boolean;data:T;error?:{message:string}}>(`aide_theme_${method}`,args);if(!r.ok)throw new Error(r.error?.message??'Theme operation failed');return r.data;}
export function ThemeManager({onChanged}:{onChanged:()=>Promise<void>}){
  const [catalog,setCatalog]=useState<Catalog>({themes:[],activeId:'default',notice:null});
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [notice,setNotice]=useState('');const [clearStorage,setClearStorage]=useState(true);
  const reload=async()=>{if(desktop)setCatalog(await request<Catalog>('list'));else setCatalog({activeId:'default',notice:null,themes:[{id:'default',name:'薄荷初音',version:'内置',builtIn:true},{id:'midnight',name:'黑暗',version:'内置',builtIn:true},{id:'paper',name:'明亮',version:'内置',builtIn:true}]});};
  useEffect(()=>{void reload().catch(e=>setError(e.message));let stop:(()=>void)|undefined;let active=true;if(desktop)void listen('aide://themes-changed',()=>{setNotice('');void reload().catch(e=>setError(e.message));void onChanged();}).then(fn=>{if(active)stop=fn;else fn();});return()=>{active=false;stop?.();};},[]);
  const run=async(method:string,args:Record<string,unknown>={})=>{setBusy(true);setError('');setNotice('');try{const result=await request<{loading?:boolean;cancelled?:boolean}>(method,args);await reload();await onChanged();if(result?.loading)setNotice(t('正在加载主题，失败时会自动恢复。'));}catch(e){setError(e instanceof Error?e.message:String(e));}finally{setBusy(false);}};
  return <section className="theme-manager"><h1>{t('外观')}</h1><p>{t('保留三套内置主题，也可安装独立的 Widget 界面。')}</p>{(error||catalog.notice||notice)&&<p className="settings-alert" role="status">{error||catalog.notice||notice}</p>}
    <div className="themes-grid">{catalog.themes.map(theme=><article className={`theme-option ${catalog.activeId===theme.id?'theme-active':''}`} key={theme.id}><div className={`theme-swatch swatch-${theme.builtIn?theme.id:'default'}`}><strong>{t(theme.name)}</strong></div><div className="theme-option-info"><span><strong>{t(theme.name)}</strong><small>{theme.builtIn?t('内置主题'):theme.version}</small></span><button disabled={!desktop||busy} onClick={()=>void run('select',{id:theme.id})}>{catalog.activeId===theme.id?t('重新加载'):t('应用')}</button></div>{!theme.builtIn&&<button className="text-button" disabled={busy} onClick={()=>void run('uninstall',{id:theme.id,clearStorage})}>{t('卸载')}</button>}</article>)}</div>
    <div className="theme-import-card"><div><strong>{t('安装 Widget 主题')}</strong><p>{t('选择 .aidetheme 包，所有资源须包含在包内。')}</p></div><button className="primary-button" disabled={!desktop||busy} onClick={()=>void run('install')}>{t('安装主题')}</button></div>
    <label><input type="checkbox" checked={clearStorage} onChange={e=>setClearStorage(e.target.checked)}/>{t('卸载时清理主题偏好')}</label>
  </section>;
}
