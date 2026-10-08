import test from 'node:test';
import assert from 'node:assert/strict';
import { nextProviderReset, resetText } from '../src/monitor-display.ts';
import type { Snapshot, AccountQuota } from '../contracts/hud-api.ts';
const now = Date.parse('2026-10-08T08:00:00Z');
const at = (minutes:number) => new Date(now + minutes*60000).toISOString();
const quota = (id:string, minutes:number, scope='base'):AccountQuota => ({ accountId:id,origin:'network',freshness:'fresh',status:'ok',lastSuccessAt:at(-1),lastAttemptAt:at(-1),observedAt:at(-1),validUntil:at(60),providerAllowed:true,baseCoverageComplete:true,blockingReason:'none',error:null,windows:[{id:'limit',scope:scope as 'base',kind:'primary',label:'5h',durationSeconds:18000,applicability:'required',measurement:'percent',remainingPercent:50,exhausted:false,resetsAt:at(minutes)}] });
const snapshot = (quotas:AccountQuota[]):Snapshot => ({accounts:[{id:'a',providerId:'codex',displayName:'a',selected:true,order:0,support:'supported'},{id:'b',providerId:'codex_usage',displayName:'b',selected:true,order:1,support:'supported'},{id:'c',providerId:'antigravity',displayName:'c',selected:true,order:2,support:'supported'}],quotas,nextRefreshAt:at(1)} as Snapshot);
test('next reset uses selected platform quota windows, not polling or another provider',()=>{
 const data=snapshot([quota('a',200),quota('b',90),quota('c',2,'feature:gemini'),quota('unselected',1)]);
 assert.equal(nextProviderReset(data,'chatgpt',now),at(90));
 assert.equal(nextProviderReset(data,'claude',now),null);
});
test('invalid, elapsed, stale and other-model resets are excluded',()=>{
 const stale=quota('a',2);stale.freshness='stale';
 assert.equal(nextProviderReset(snapshot([stale,quota('b',-1)]),'chatgpt',now),null);
 const data=quota('c',120,'feature:gemini');data.windows.push({...data.windows[0],id:'claude',scope:'feature:claude',resetsAt:at(5)});
 assert.equal(nextProviderReset(snapshot([data]),'antigravity',now),at(120));
 assert.equal(resetText(at(-1),now),'—');assert.equal(resetText('bad',now),'—');assert.equal(resetText(at(90),now),'1h30m');
});
