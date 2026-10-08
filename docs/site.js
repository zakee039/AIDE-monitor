'use strict';
const platformData={codex:['ChatGPT / Codex','短周期与周额度，各自清楚显示。'],antigravity:['Antigravity','内置面板展示 Gemini 额度与重置时间。'],claude:['Claude','额度偏低或耗尽，提前看见。'],grok:['Grok','不同额度项目，集中呈现。']};
const monitorData={'1account':['01','两项额度，左右并排','账号数量不同，布局随之适应。'],'2account':['02','上下两行，清楚对照','每个账号的额度与重置时间，各有位置。'],'3account':['03','自动分栏，一目了然','三个账号并排展示，关注各自的进度。'],'4account':['04','四列展示，一屏掌握','更多账号每 8 秒翻页，逐页查看。'],'4plat':['05','四大平台，分别汇总','总额度、最近重置倒计时与参与展示的账号数量。']};
function choose(group,selected){group.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b===selected)))}
document.querySelectorAll('[data-provider]').forEach(b=>b.addEventListener('click',()=>{const id=b.dataset.provider;choose(b.parentElement,b);document.querySelector('#platform-label').textContent=platformData[id][0];document.querySelector('#platform-detail').textContent=platformData[id][1];const img=document.querySelector('#platform-shot');img.src='assets/product/platform-'+id+'.png';img.alt=platformData[id][0]+' 演示账号额度界面'}));
document.querySelectorAll('[data-monitor]').forEach(b=>b.addEventListener('click',()=>{const id=b.dataset.monitor;choose(b.parentElement,b);const [number,title,detail]=monitorData[id];document.querySelector('#monitor-number').textContent=number;document.querySelector('#monitor-title').textContent=title;document.querySelector('#monitor-detail').textContent=detail;const img=document.querySelector('#monitor-shot');img.src='assets/product/'+id+'.png';img.alt='独立监视屏'+b.textContent+'布局设备实拍'}));
document.querySelectorAll('[data-settings]').forEach(b=>b.addEventListener('click',()=>{choose(b.parentElement,b);document.querySelector('#settings-label').textContent='AIDE monitor / '+b.textContent;const img=document.querySelector('#settings-shot');img.src='assets/product/settings-'+b.dataset.settings+'.png';img.alt=b.textContent+'设置界面'}));
// Bilingual native content and reference-style click navigation.
const translations={
'额度看得见，':'Quota in sight,','专注':'focus ','不断线。':'uninterrupted.','不用来回打开每个平台。':'No more checking one app after another.','把 AI 账号的额度，轻轻放在眼前。':'Keep your AI account quota in view.','大平台':' platforms','一个小窗口':'One small window','多账号 · 剩余额度 · 重置时间':'Multiple accounts · Quota · Reset times','下载 Windows 版 ↗':'Download for Windows ↗','继续了解 →':'Explore more →','下载 ↗':'Download ↗','Windows x64 · 桌面悬浮窗与独立监视屏':'Windows x64 · Desktop widget & dedicated display','一点薄荷，一目了然。':'A little mint. A clearer view.','你的额度小伙伴。':'Your little quota companion.','轻轻收起，':'Tuck it away,','重点还在。':'keep what matters.',
'四大平台，':'Four platforms,','一处':'one ','查看。':'clear view.','哪个账号还有额度，什么时候重置。':'Know which account has quota, and when it resets.','看一眼，心里就有数。':'One glance is all it takes.','短周期与周额度，各自清楚显示。':'Short-term and weekly quotas, clearly displayed.','内置面板展示 Gemini 额度与重置时间。':'View Gemini quota and reset times.','额度偏低或耗尽，提前看见。':'Spot low or exhausted quota at a glance.','不同额度项目，集中呈现。':'Different quota limits, together in one view.','额度预览':'Quota preview','界面演示 · 账号与额度均为虚构':'Demo interface · Fictional accounts and quota','清楚，不复杂。':'Clear. Uncomplicated.',
'把额度，放到':'Give your quota','独立小屏上。':'its own screen.','工作留在主屏，额度自有位置。':'Your work on the main screen. Quota on its own.','一块桌面小屏，让状态随时可见。':'A small desktop display keeps you informed.','单账号':'1 account','双账号':'2 accounts','三账号':'3 accounts','四账号':'4 accounts','四平台':'4 platforms','两项额度，左右并排':'Two quotas, side by side','账号数量不同，布局随之适应。':'The layout adapts to your account count.','上下两行，清楚对照':'Two rows, easy to compare','每个账号的额度与重置时间，各有位置。':'Quota and reset times have their own place.','自动分栏，一目了然':'Automatic columns, instant clarity','三个账号并排展示，关注各自的进度。':'Three accounts side by side.','四列展示，一屏掌握':'Four columns, one clear view','更多账号每 8 秒翻页，逐页查看。':'More accounts cycle through pages every 8 seconds.','四大平台，分别汇总':'Four platforms, separate totals','总额度、最近重置倒计时与参与展示的账号数量。':'Total quota, next reset and selected account count.','独立主题 · 自适应布局':'Separate themes · Adaptive layout','桌面上的一小块安心。':'A little peace of mind on your desk.','设备实拍':'Actual device photo',
'照你的习惯，':'Your habits,','刚刚':'your ','好。':'way.','想看哪些账号，多久刷新一次。':'Choose your accounts and refresh frequency.','每一个小习惯，都有自己的位置。':'Make room for the way you work.','账号管理':'Accounts','窗口与显示':'Window & display','每个账号，单独安排':'Set up each account your way','勾选展示 · 独立代理 · 刷新频率':'Selection · Proxies · Refresh frequency','舒服的窗口，顺手的设置':'A window that works for you','置顶 · 位置锁定 · 隐私模式':'Always on top · Lock position · Privacy mode','是喜欢的样子呀 ♡':'Just the way you like it ♡','设置预览':'Settings preview','AIDE monitor / 账号管理':'AIDE monitor / Accounts','AIDE monitor / 窗口与显示':'AIDE monitor / Window & display',
'少一点查询，':'Less checking,','多一点专注。':'more creating.','让额度清楚可见。':'Keep your quota in sight.','接下来，好好创作。':'Now, get back to what you love.','下载 AIDE monitor ↗':'Download AIDE monitor ↗','使用说明 ↗':'Getting started ↗','前往 GitHub Releases，选择安装包或便携版。':'Choose an installer or portable version on GitHub Releases.'
};
const reverse=Object.fromEntries(Object.entries(translations).map(([a,b])=>[b.trim(),a]));
let language=localStorage.getItem('aide-site-language')==='en'?'en':'zh-CN';
const scenes=[...document.querySelectorAll('.scene')];let page=0;
function fitScene(){
 const el=scenes[page];el.style.zoom='';
 const available=document.querySelector('main').clientHeight;
 if(el.scrollHeight>available+2)el.style.zoom=Math.min(1,available/el.scrollHeight);
}
function translate(){
 document.documentElement.lang=language;
 const walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);let node;
 while(node=walker.nextNode()){
  if(node.parentElement.closest('script,style,.language-switch'))continue;
  const value=node.nodeValue.trim();const map=language==='en'?translations:reverse;
  if(Object.hasOwn(map,value))node.nodeValue=map[value];
 }
 document.querySelectorAll('[data-lang]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.lang===language)));
 document.title=language==='en'?'AIDE monitor · Your quota. In sight.':'AIDE monitor · 让额度清楚可见';
 requestAnimationFrame(fitScene);
}
function showPage(n,update=true){
 page=Math.max(0,Math.min(scenes.length-1,n));
 scenes.forEach((s,i)=>{s.classList.toggle('active',i===page);s.inert=i!==page;s.setAttribute('aria-hidden',String(i!==page))});
 if(update)history.replaceState(null,'','#'+scenes[page].id);
 requestAnimationFrame(fitScene);
}
function fromHash(){const i=scenes.findIndex(s=>s.id===location.hash.slice(1));showPage(i<0?0:i,false)}
let origin=null;
const main=document.querySelector('main');
main.addEventListener('pointerdown',e=>origin={x:e.clientX,y:e.clientY});
main.addEventListener('click',e=>{
 if(e.target.closest('button,a')||getSelection()?.toString()||(origin&&Math.hypot(e.clientX-origin.x,e.clientY-origin.y)>10))return;
 showPage(page+1);
});
document.querySelectorAll('[data-lang]').forEach(b=>b.onclick=()=>{language=b.dataset.lang;localStorage.setItem('aide-site-language',language);translate()});
document.querySelectorAll('[data-provider],[data-monitor],[data-settings]').forEach(b=>b.addEventListener('click',()=>{translate();}));
document.querySelectorAll('a[href^="#"]').forEach(a=>a.addEventListener('click',e=>{e.preventDefault();const i=scenes.findIndex(s=>'#'+s.id===a.getAttribute('href'));if(i>=0)showPage(i)}));
addEventListener('keydown',e=>{if(e.target.closest('button,a,input'))return;if(['ArrowRight','ArrowLeft',' '].includes(e.key)){e.preventDefault();showPage(page+(e.key==='ArrowLeft'?-1:1))}});
addEventListener('hashchange',fromHash);addEventListener('resize',fitScene);document.querySelectorAll('img').forEach(img=>img.addEventListener('load',fitScene));
fromHash();translate();
