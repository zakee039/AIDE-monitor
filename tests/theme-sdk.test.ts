import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

function harness() {
  const events = new Map(); const requests: any[] = []; const polls: Function[] = [];
  const window: any = { addEventListener: (name: string, fn: Function) => events.set(name, fn), fetch: (_: string, options: any) => new Promise(resolve => requests.push({body: JSON.parse(options.body), signal: options.signal, resolve})) };
  vm.runInNewContext(readFileSync(new URL('../theme-sdk/bootstrap.js', import.meta.url), 'utf8'), {window, AbortController, setTimeout, clearTimeout, setInterval: (fn: Function) => polls.push(fn), clearInterval() {}});
  const reply = (index: number, revision: number, instanceId = 'host', overrides = {}) => { const r = requests[index]; r.resolve({json: async () => ({ok: true, apiVersion: '1', requestId: r.body.requestId, sessionId: r.body.sessionId, revision, sequence: revision, instanceId, data: {revision}, ...overrides})}); };
  return {window, requests, polls, events, reply};
}
test('subscriptions reject late snapshots, recover gaps, and unsubscribe', async () => {
  const h = harness(); const seen: number[] = [];
  const pending = h.window.aide.usage.onChanged((s: any) => seen.push(s.revision));
  const poll = h.polls[0](); h.reply(1, 4); await poll;
  h.reply(0, 2); const stop = await pending;
  assert.deepEqual(seen, [4]);
  const gap = h.polls[0](); h.reply(2, 9); await gap;
  const restart = h.polls[0](); h.reply(3, 1, 'new-host'); await restart;
  stop(); const after = h.polls[0](); h.reply(4, 2, 'new-host'); await after;
  assert.deepEqual(seen, [4, 9, 1]);
});
test('SDK checks correlation and closes pending work with the page', async () => {
  const h = harness();
  const request = h.window.aide.usage.get(); h.reply(0, 1, 'host', {requestId: 'wrong'});
  await assert.rejects(request, /INVALID_REPLY/);
  const pending = h.window.aide.accounts.list();
  h.events.get('pagehide')(); assert.equal(h.requests[1].signal.aborted, true);
  h.reply(1, 1); await pending;
  await assert.rejects(h.window.aide.usage.get(), /SESSION_CLOSED/);
  assert.equal(Object.getOwnPropertyDescriptor(h.window, 'aide')?.writable, false);
});
