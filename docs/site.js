'use strict';
const $=selector=>document.querySelector(selector);
const chapters=['概览','多账号','四平台','账号管理','窗口设置','外观','悬浮球','独立监视屏','开始使用'];
let cuts=[],current=0;
function go(index,hash=true){
  current=Math.max(0,Math.min(cuts.length-1,index));const cut=cuts[current];if(!cut)return;
  $('#slide').src=cut.image;$('#slide').alt=cut.alt;$('#caption').textContent=cut.caption;
  $('#counter').textContent=String(cut.page+1).padStart(2,'0')+' / 09';
  $('#kind').textContent=cut.page===7?'独立监视屏 · 实拍':'界面演示 · 虚构数据';
  $('#previous').disabled=current===0;$('#next').disabled=current===cuts.length-1;
  $('#scene').setAttribute('aria-label',current===cuts.length-1?'回到第一张展示画面':'下一张：'+cuts[current+1].title);
  document.querySelectorAll('.chapter-nav button').forEach((b,i)=>b.setAttribute('aria-current',String(i===cut.page)));
  if(hash)history.replaceState(null,'','#'+(current+1));
  const next=cuts[current+1];if(next){const img=new Image();img.src=next.image;}
}
$('#previous').onclick=()=>go(current-1);$('#next').onclick=()=>go(current+1);
let startX=0,startY=0;
$('#scene').addEventListener('pointerdown',e=>{startX=e.clientX;startY=e.clientY});
$('#scene').addEventListener('click',e=>{if(e.detail&&Math.hypot(e.clientX-startX,e.clientY-startY)>12)return;go(current===cuts.length-1?0:current+1)});
addEventListener('keydown',e=>{if(e.target.closest('a,button,input,video'))return;if(e.key==='ArrowRight'||e.key==='ArrowLeft'){e.preventDefault();go(current+(e.key==='ArrowRight'?1:-1))}});
addEventListener('hashchange',()=>go((parseInt(location.hash.slice(1),10)||1)-1,false));
fetch('showcase.json').then(r=>{if(!r.ok)throw Error('load');return r.json()}).then(data=>{
  cuts=data;chapters.forEach((label,page)=>{const b=document.createElement('button');b.textContent=String(page+1).padStart(2,'0')+' '+label;b.onclick=()=>go(cuts.findIndex(c=>c.page===page));$('.chapter-nav').append(b)});
  go((parseInt(location.hash.slice(1),10)||1)-1,false);
  $('.hint').textContent=matchMedia('(max-width:700px)').matches?'横向滑动查看完整画面 · 点击箭头翻页':'点击画面继续 · 支持 ← → 方向键';
}).catch(()=>{$('#caption').textContent='画面暂时未能加载，请刷新后重试。';$('#next').disabled=true;$('#previous').disabled=true});
