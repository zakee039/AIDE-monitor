import test from 'node:test';
import assert from 'node:assert/strict';
import { quotaBandForPercent } from '../src/quota-colors.ts';
test('desktop and monitor share quota thresholds, including totals over 100%',()=>{
 for(const [value,color] of [[0,'red'],[19.9,'red'],[20,'orange'],[49.9,'orange'],[50,'blue'],[79.9,'blue'],[80,'green'],[185,'green']] as const) assert.equal(quotaBandForPercent(value),`quota-${color}`);
 for(const value of [null,undefined,NaN])assert.equal(quotaBandForPercent(value),'');
});
