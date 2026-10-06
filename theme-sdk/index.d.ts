import type { Snapshot, AccountSummary, RefreshTicket } from './contracts';
export interface Aide {
  accounts: { list(): Promise<AccountSummary[]>; refresh(params?: {accountIds?: string[]}): Promise<RefreshTicket> };
  usage: { get(): Promise<Snapshot>; onChanged(listener:(snapshot:Snapshot)=>void): Promise<()=>void> };
  window: { resize(width:number,height:number):Promise<{width:number;height:number}>; drag():Promise<unknown>; hide():Promise<unknown> };
  storage: { get(key:string):Promise<unknown>; set(key:string,value:unknown):Promise<unknown>; remove(key:string):Promise<unknown> };
  app: { openSettings():Promise<unknown> };
  theme: { ready():Promise<unknown> };
}
export const aide:Aide;
declare global { interface Window { readonly aide:Aide } }
