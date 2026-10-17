/* Veil VPN 1.5.1 — shared app logic (Android bridge / Windows local API / web demo) */
(function(){
'use strict';

/* ---------------- bridge ---------------- */
var BRIDGE = {
  platform: 'web',
  native: null,
  base: '',
  cbs: {}, cbn: 0,
  detect: function(){
    if (window.VeilNative && typeof window.VeilNative.api === 'function'){
      this.platform='android'; this.native=window.VeilNative; return;
    }
    if (window.__VEIL_WIN__){ this.platform='windows'; this.base=window.__VEIL_WIN__.base||''; return; }
    this.platform='web';
  },
  call: function(action, payload){
    var self=this;
    payload = payload||{};
    if (this.platform==='android'){
      return new Promise(function(res,rej){
        var id = 'cb'+(++self.cbn);
        self.cbs[id]={res:res,rej:rej};
        try { self.native.api(action, JSON.stringify(payload), id); }
        catch(e){ delete self.cbs[id]; rej(e); }
        setTimeout(function(){ if(self.cbs[id]){ delete self.cbs[id]; rej(new Error('timeout')); } }, 30000);
      });
    }
    return fetch(this.base+'api/'+action, {
      method:'POST', headers:{'Content-Type':'application/json'},
      body: JSON.stringify(payload)
    }).then(function(r){ return r.json(); });
  },
  resolve: function(cbId, json){ var c=this.cbs[cbId]; if(c){ delete this.cbs[cbId]; try{ c.res(JSON.parse(json)); }catch(e){ c.res({}); } } },
  reject: function(cbId, msg){ var c=this.cbs[cbId]; if(c){ delete this.cbs[cbId]; c.rej(new Error(msg)); } }
};
window.__nCb = function(id, json){ BRIDGE.resolve(id, json); };
window.__nErr = function(id, msg){ BRIDGE.reject(id, msg); };
window.__nEvent = function(kind, json){
  try{
    var d = typeof json === 'string' ? JSON.parse(json) : json;
    if(kind === 'servers') onServers(d);
    else if(kind === 'status') onStatus(d);
  }catch(e){}
};

/* ---------------- country table ---------------- */
var C = {US:['United States','New York'],DE:['Germany','Frankfurt'],GB:['United Kingdom','London'],NL:['Netherlands','Amsterdam'],
FR:['France','Paris'],CA:['Canada','Toronto'],SG:['Singapore','Singapore'],JP:['Japan','Tokyo'],TR:['Türkiye','Istanbul'],
AE:['UAE','Dubai'],FI:['Finland','Helsinki'],SE:['Sweden','Stockholm'],CH:['Switzerland','Zürich'],AT:['Austria','Vienna'],
PL:['Poland','Warsaw'],IE:['Ireland','Dublin'],IT:['Italy','Milan'],ES:['Spain','Madrid'],PT:['Portugal','Lisbon'],
RU:['Russia','Moscow'],UA:['Ukraine','Kyiv'],KZ:['Kazakhstan','Almaty'],IN:['India','Mumbai'],HK:['Hong Kong','Hong Kong'],
TW:['Taiwan','Taipei'],KR:['South Korea','Seoul'],CN:['China','Beijing'],AU:['Australia','Sydney'],NZ:['New Zealand','Auckland'],
BR:['Brazil','São Paulo'],AR:['Argentina','Buenos Aires'],MX:['Mexico','Mexico City'],CL:['Chile','Santiago'],
ZA:['South Africa','Cape Town'],EG:['Egypt','Cairo'],IL:['Israel','Tel Aviv'],RO:['Romania','Bucharest'],
BG:['Bulgaria','Sofia'],HU:['Hungary','Budapest'],CZ:['Czechia','Prague'],SK:['Slovakia','Bratislava'],
LT:['Lithuania','Vilnius'],LV:['Latvia','Riga'],EE:['Estonia','Tallinn'],NO:['Norway','Oslo'],DK:['Denmark','Copenhagen'],
IS:['Iceland','Reykjavík'],LU:['Luxembourg','Luxembourg'],MD:['Moldova','Chișinău'],RS:['Serbia','Belgrade'],
TH:['Thailand','Bangkok'],VN:['Vietnam','Hanoi'],PH:['Philippines','Manila'],MY:['Malaysia','Kuala Lumpur'],
ID:['Indonesia','Jakarta'],PK:['Pakistan','Karachi'],BD:['Bangladesh','Dhaka'],IQ:['Iraq','Baghdad'],
SA:['Saudi Arabia','Riyadh'],QA:['Qatar','Doha'],KW:['Kuwait','Kuwait City'],GE:['Georgia','Tbilisi'],
AM:['Armenia','Yerevan'],AZ:['Azerbaijan','Baku'],GR:['Greece','Athens'],CY:['Cyprus','Nicosia'],ED:['England','London']};
function ccName(cc){ return (C[cc]||[cc||'Veil','Global'])[0]; }
function ccCity(cc){ return (C[cc]||['Veil','Global'])[1]; }
function ccFlag(cc){
  if(!cc||cc.length!==2) return '🛡️';
  var a=cc.charCodeAt(0),b=cc.charCodeAt(1);
  if(a<65||a>90||b<65||b>90) return '🛡️';
  return String.fromCodePoint(0x1F1E6+(a-65))+String.fromCodePoint(0x1F1E6+(b-65));
}

/* ---------------- state ---------------- */
var state = {
  status:'disconnected', servers:[], selected:null, seconds:0, timers:[],
  lic:null, features:{kill:true,auto:true,adblock:false}, lang:null,
  ipInfo:null, bootReady:false, lastError:null
};

var app=document.getElementById('app'),
  connectBtn=document.getElementById('connectBtn'),
  connectTitle=document.getElementById('connectTitle'),
  connectSub=document.getElementById('connectSub'),
  statusPill=document.getElementById('statusPill'),
  statusText=document.getElementById('statusText'),
  serverList=document.getElementById('serverList'),
  serverHint=document.getElementById('serverHint'),
  statEls={ping:document.querySelector('[data-key="ping"]'),down:document.querySelector('[data-key="down"]'),up:document.querySelector('[data-key="up"]')},
  toastEl=document.getElementById('toast'),
  ipLine=document.getElementById('ipLine'), ipText=document.getElementById('ipText');

/* ---------------- i18n ---------------- */
function detectLang(){
  var saved=null;
  try{ saved=localStorage.getItem('veil.lang'); }catch(e){}
  if(saved) return saved;
  return 'en'; /* default language: English (user can pick any in settings) */
}
function applyLang(lang){
  state.lang=lang;
  try{ localStorage.setItem('veil.lang',lang); }catch(e){}
  window.__veilLang=lang;
  document.documentElement.lang=lang;
  document.documentElement.dir = window.I18N_RTL[lang]?'rtl':'ltr';
  document.querySelectorAll('[data-i18n]').forEach(function(el){
    var k=el.getAttribute('data-i18n');
    el.textContent = window.t(k);
  });
  document.querySelectorAll('#langSel option').forEach(function(o){ if(o.value===lang) o.selected=true; });
  refreshStaticText();
}
function refreshStaticText(){
  if(state.status==='disconnected') updateIdleUI();
  statusText.textContent = window.t(state.status==='connected'?'protected':state.status==='connecting'?'connecting':'disconnected');
  if(state.lic) renderLicBadges();
}

/* ---------------- helpers ---------------- */
var rand=function(a,b){ return Math.random()*(b-a)+a; };
function toast(msg, err){
  toastEl.textContent=msg; toastEl.classList.toggle('err', !!err); toastEl.classList.add('show');
  clearTimeout(toastEl._t); toastEl._t=setTimeout(function(){ toastEl.classList.remove('show'); }, 2600);
}
function setValue(key, value, unit){
  var el=statEls[key]; if(!el) return;
  el.innerHTML = unit? value+'<small>'+unit+'</small>' : value;
}
function bump(el){ if(!el) return; el.classList.remove('pulse'); void el.offsetWidth; el.classList.add('pulse'); }
function fmtTime(s){
  var h=String(Math.floor(s/3600)).padStart(2,'0'), m=String(Math.floor((s%3600)/60)).padStart(2,'0'), x=String(s%60).padStart(2,'0');
  return h+':'+m+':'+x;
}
function pingClass(ms){ return ms<50?'good':ms<120?'medium':'far'; }
function esc(s){ return String(s==null?'':s).replace(/[&<>"']/g,function(c){return{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c];}); }

/* ---------------- servers ---------------- */
function renderServers(){
  serverList.innerHTML='';
  if(!state.servers.length){
    serverList.innerHTML='<div style="padding:16px;text-align:center;color:rgba(255,255,255,.35);font-size:12px">'+(state.status==='connecting'?'···':esc(window.t('no_servers')))+'</div>';
    serverHint.textContent=window.t('no_servers');
    return;
  }
  var frag=document.createDocumentFragment();
  state.servers.slice(0,120).forEach(function(s){
    var b=document.createElement('button');
    b.className='server'+(state.selected&&s.id===state.selected.id?' active':'');
    b.type='button'; b.dataset.id=s.id;
    var nm=esc(s.name||ccName(s.cc)), ct=esc(s.city||ccCity(s.cc));
    var pg=s.ping!=null? '<span class="server-ping '+pingClass(s.ping)+'"><span class="ping-dot"></span>'+s.ping+' ms</span>'
                        : '<span class="server-ping"><span class="ping-dot"></span>···</span>';
    b.innerHTML='<span class="server-flag">'+ccFlag(s.cc)+'</span>'+
      '<span class="server-info"><span class="server-name">'+nm+'</span><span class="server-city">'+ct+' · '+esc(s.p||'')+'</span></span>'+pg;
    b.addEventListener('click',function(){ selectServer(s.id); });
    frag.appendChild(b);
  });
  serverList.appendChild(frag);
  serverHint.textContent = state.servers.length+' '+window.t('locations');
}
function selectServer(id){
  var next=state.servers.find(function(s){return s.id===id;});
  if(!next) return;
  state.selected=next;
  try{ localStorage.setItem('veil.srv', id); }catch(e){}
  renderServers();
  if(state.status==='connected'){ doDisconnect(true); connect(); }
  else if(state.status==='disconnected'){ updateIdleUI(); }
}
function onServers(list){
  state.servers=list||[];
  if(!state.servers.length){ renderServers(); return; }
  var saved=null;
  try{ saved=localStorage.getItem('veil.srv'); }catch(e){}
  var sel = state.selected && state.servers.find(function(s){return s.id===state.selected.id;});
  if(!sel && saved) sel=state.servers.find(function(s){return s.id===saved;});
  if(!sel) sel=state.servers.find(function(s){return s.ping!=null;}) || state.servers[0];
  state.selected=sel;
  renderServers();
  if(state.bootReady) toast(state.servers.length+' '+window.t('servers_loaded'));
  var need=state.servers.filter(function(s){return s.ping==null;}).slice(0,60).map(function(s){return s.id;});
  if(need.length) BRIDGE.call('pingBatch',{ids:need}).then(function(r){
    if(!r||!r.pings) return;
    state.servers.forEach(function(s){ if(r.pings[s.id]!=null) s.ping=r.pings[s.id]; });
    renderServers();
  }).catch(function(){});
}

/* ---------------- status / connect ---------------- */
function applyStatusClass(){
  app.classList.remove('connected','connecting');
  if(state.status==='connected') app.classList.add('connected');
  if(state.status==='connecting') app.classList.add('connecting');
  statusPill.classList.remove('connected','connecting');
  if(state.status==='connected') statusPill.classList.add('connected');
  if(state.status==='connecting') statusPill.classList.add('connecting');
}
function updateIdleUI(){
  connectTitle.textContent=window.t('tap_connect');
  var sel=state.selected;
  connectSub.innerHTML = sel? window.t('selected')+': <span style="color:rgba(255,255,255,.7);font-weight:500">'+ccFlag(sel.cc)+' '+esc(ccName(sel.cc))+'</span>' : esc(window.t('slogan'));
  statusText.textContent=window.t('disconnected');
  setValue('ping','--','ms'); setValue('down','--','Mb'); setValue('up','--','Mb');
  ipLine.style.display='none';
}
function clearTimers(){ state.timers.forEach(clearInterval); state.timers=[]; }
function startLiveData(){
  clearTimers(); state.seconds=0;
  state.timers.push(setInterval(function(){
    state.seconds++;
    connectSub.innerHTML = window.t('connected')+' · <span class="timer">'+fmtTime(state.seconds)+'</span>';
  },1000));
  state.timers.push(setInterval(function(){
    var base=(state.selected&&state.selected.ping)||30;
    setValue('ping', Math.max(8, Math.round(base+rand(-4,6))), 'ms'); bump(statEls.ping);
  },2000));
  state.timers.push(setInterval(function(){ setValue('down', rand(18,86).toFixed(1),'Mb'); bump(statEls.down); },1700));
  state.timers.push(setInterval(function(){ setValue('up', rand(6,32).toFixed(1),'Mb'); bump(statEls.up); },2100));
}
function connect(){
  if(state.status!=='disconnected') return;
  if(!state.selected){ toast(window.t('no_servers'), true); return; }
  state.status='connecting'; applyStatusClass();
  connectTitle.textContent=window.t('connecting');
  connectSub.textContent=window.t('securing')+' · '+ccFlag(state.selected.cc)+' '+ccName(state.selected.cc);
  statusText.textContent=window.t('connecting');
  setValue('ping','···',''); setValue('down','···',''); setValue('up','···','');
  BRIDGE.call('connect',{id:state.selected.id}).then(function(r){
    if(r && r.ok && r.async) return;
    if(r && r.ok && r.handoff){ toast(window.t('connecting')); return; }
    if(r && !r.ok && r.error==='activate'){ state.status='disconnected'; applyStatusClass(); updateIdleUI(); showGate(); return; }
    if(r && !r.ok){ state.status='disconnected'; applyStatusClass(); updateIdleUI(); toast(r.error||window.t('conn_fail'), true); }
  }).catch(function(e){
    state.status='disconnected'; applyStatusClass(); updateIdleUI();
    toast(window.t('conn_fail'), true);
  });
}
function doDisconnect(instant){
  if(state.status==='disconnected' && instant) return;
  clearTimers(); state.seconds=0;
  var finish=function(){ state.status='disconnected'; applyStatusClass(); updateIdleUI(); };
  if(instant){ finish(); return; }
  state.status='disconnecting';
  connectTitle.textContent=window.t('disconnecting');
  connectSub.textContent=window.t('disconnected');
  statusText.textContent=window.t('disconnecting');
  BRIDGE.call('disconnect',{}).catch(function(){});
  setTimeout(finish, 500);
}
function onStatus(st){
  var prev=state.status;
  state.status=st.state;
  state.lastError=st.error||null;
  if(st.server){ var m=state.servers.find(function(s){return s.id===st.server.id;}); if(m) state.selected=m; else { state.selected=Object.assign({cc:'XX'},st.server); } }
  applyStatusClass();
  if(state.status==='connected'){
    connectTitle.textContent=window.t('connected');
    statusText.textContent=window.t('protected');
    startLiveData();
    loadIpInfo();
  } else if(state.status==='connecting'){
    connectTitle.textContent=window.t('connecting');
    statusText.textContent=window.t('connecting');
  } else if(state.status==='reconnecting'){
    connectTitle.textContent=window.t('connecting');
    statusText.textContent=window.t('connecting');
  } else if(state.status==='disconnected'){
    clearTimers();
    if(prev==='connected'||prev==='connecting'||prev==='reconnecting'){
      toast(st.error||window.t('disconnected'), !!st.error);
      BRIDGE.call('disconnect',{}).catch(function(){});
    }
    updateIdleUI();
  }
}
function loadIpInfo(){
  ipLine.style.display='none';
  BRIDGE.call('getIpInfo',{}).then(function(r){
    if(r&&r.ip){ state.ipInfo=r; ipLine.style.display='flex';
      ipText.innerHTML=window.t('via')+' <b>'+esc(r.ip)+'</b>'+(r.cc?' · '+ccFlag(r.cc)+' '+esc(ccName(r.cc)):''); }
  }).catch(function(){});
}

/* ---------------- license gate ---------------- */
var gateOverlay=document.getElementById('gateOverlay'),
  settingsOverlay=document.getElementById('settingsOverlay'),
  licBadge=document.getElementById('licBadge'), gateLicBadge=document.getElementById('gateLicBadge');
function licText(lic){
  if(!lic) return '';
  var t = lic.type==='pre'? window.t('premium') : window.t('free');
  var d = lic.type==='pre'? window.t('lifetime') : (lic.daysLeft>0? lic.daysLeft+' '+window.t('days_left') : window.t('expired'));
  return {t:t,d:d};
}
function renderLicBadges(){
  var L=state.lic; if(!L) return;
  var x=licText(L);
  var html='<div><div class="t">'+x.t+'</div><div class="d">'+x.d+' · '+esc(window.t('trial_note'))+': '+L.device+'</div></div>'+
    '<span class="tag'+(L.type==='pre'?' pre':'')+'">'+(L.type==='pre'?'PREMIUM':'FREE')+'</span>';
  licBadge.innerHTML=html;
  gateLicBadge.innerHTML=html;
}
function showGate(){
  renderLicBadges();
  gateOverlay.classList.add('show');
}
function hideGate(){ gateOverlay.classList.remove('show'); }
function activate(){
  var inp=document.getElementById('gateKey');
  var key=(inp.value||'').trim().toUpperCase();
  if(!key){ toast(window.t('need_key'), true); return; }
  BRIDGE.call('activateKey',{key:key}).then(function(r){
    if(r&&r.ok){
      toast(window.t('key_ok'));
      inp.value='';
      state.lic=r.lic||null; renderLicBadges();
      hideGate(); settingsOverlay.classList.remove('show');
      if(state.status==='disconnected') connect();
    } else toast((r&&r.error)||window.t('key_bad'), true);
  }).catch(function(){ toast(window.t('key_bad'), true); });
}

/* ---------------- settings ---------------- */
function fillLangs(){
  var sel=document.getElementById('langSel');
  sel.innerHTML = window.I18N_LIST.map(function(l){ return '<option value="'+l[0]+'">'+l[1]+'</option>'; }).join('');
  sel.value=state.lang||detectLang();
}
function openSettings(){
  BRIDGE.call('licenseInfo',{}).then(function(r){
    if(r){ state.lic=r; renderLicBadges(); }
    document.getElementById('srcState').textContent='';
  }).catch(function(){});
  fillLangs();
  try{ document.getElementById('srcInput').value = localStorage.getItem('veil.src')||''; }catch(e){}
  settingsOverlay.classList.add('show');
}
document.getElementById('settingsBtn').addEventListener('click', openSettings);
document.getElementById('setClose').addEventListener('click', function(){ settingsOverlay.classList.remove('show'); });
document.getElementById('langSel').addEventListener('change', function(e){
  applyLang(e.target.value);
  BRIDGE.call('setLang',{lang:e.target.value}).catch(function(){});
});
document.getElementById('srcSave').addEventListener('click', function(){
  var u=(document.getElementById('srcInput').value||'').trim();
  try{ localStorage.setItem('veil.src', u); }catch(e){}
  toast(window.t('refreshing'));
  BRIDGE.call('setSource',{url:u}).then(function(){ return BRIDGE.call('fetchServers',{}); }).then(function(r){ if(r&&r.servers) onServers(r.servers); }).catch(function(){});
});
document.getElementById('refreshServers').addEventListener('click', function(){
  toast(window.t('refreshing'));
  BRIDGE.call('fetchServers',{}).then(function(r){ if(r&&r.servers) onServers(r.servers); }).catch(function(){});
});
document.getElementById('setDeactivate').addEventListener('click', function(){
  BRIDGE.call('deactivateKey',{}).then(function(){ toast(window.t('deactivated')); settingsOverlay.classList.remove('show'); state.lic=null; }).catch(function(){});
});
var BOT='https://t.me/veilvpnverifybot';
function openBot(){
  var url=BOT;
  BRIDGE.call('openUrl',{url:url}).catch(function(){
    try{ window.open(url,'_blank'); }catch(e){ location.href=url; }
  });
}
document.getElementById('gateBotBtn').addEventListener('click', openBot);
document.getElementById('setBotBtn').addEventListener('click', openBot);
document.getElementById('gateActivate').addEventListener('click', activate);
document.getElementById('gateKey').addEventListener('keydown', function(e){ if(e.key==='Enter') activate(); });
document.getElementById('gateLater').addEventListener('click', function(){ hideGate(); });
gateOverlay.addEventListener('click', function(e){ if(e.target===gateOverlay) hideGate(); });
settingsOverlay.addEventListener('click', function(e){ if(e.target===settingsOverlay) settingsOverlay.classList.remove('show'); });

/* ---------------- features ---------------- */
document.querySelectorAll('.feature').forEach(function(btn){
  btn.addEventListener('click', function(){
    var k=btn.dataset.feature;
    var on=!btn.classList.contains('on');
    btn.classList.toggle('on',on);
    state.features[k]=on;
    BRIDGE.call('setFeature',{key:k,on:on}).catch(function(){});
  });
});

/* ---------------- windows event poll ---------------- */
function winEvents(){
  if(BRIDGE.platform!=='windows') return;
  fetch(BRIDGE.base+'api/events').then(function(r){return r.json();}).then(function(ev){
    if(ev){
      if(ev.kind==='status') onStatus(ev.state);
      else if(ev.kind==='servers') onServers(ev.servers);
    }
  }).catch(function(){}).finally(function(){ setTimeout(winEvents, 300); });
}

/* ---------------- connect btn ---------------- */
connectBtn.addEventListener('click', function(){
  if(state.status==='disconnected') connect();
  else if(state.status==='connected') doDisconnect(false);
});

/* ---------------- boot ---------------- */
function boot(){
  BRIDGE.detect();
  document.getElementById('verMark').textContent='v1.5.1 · '+BRIDGE.platform;
  state.lang=detectLang();
  applyLang(state.lang);
  renderServers();
  applyStatusClass();
  updateIdleUI();
  BRIDGE.call('getState',{}).then(function(g){
    if(!g) return;
    state.platform=g.platform;
    if(g.features){ state.features=g.features;
      document.querySelectorAll('.feature').forEach(function(b){ b.classList.toggle('on', !!g.features[b.dataset.feature]); }); }
    if(g.lang){ applyLang(g.lang); fillLangs(); }
    if(g.lic){ state.lic=g.lic; renderLicBadges(); }
    if(g.status && g.status!=='disconnected'){
      state.status=g.status;
      if(g.server){ state.selected=Object.assign({cc:'XX'},g.server); }
      applyStatusClass();
      if(g.status==='connected'){ startLiveData(); loadIpInfo(); connectTitle.textContent=window.t('connected'); statusText.textContent=window.t('protected'); }
      else { connectTitle.textContent=window.t('connecting'); statusText.textContent=window.t('connecting'); }
    }
    state.bootReady=true;
    return BRIDGE.call('fetchServers',{});
  }).then(function(r){
    if(r&&r.servers&&r.servers.length){ onServers(r.servers); }
    else { if(BRIDGE.platform==='web'){ onServers(DEMO); } }
  }).catch(function(){ if(BRIDGE.platform==='web') onServers(DEMO); });
  if(BRIDGE.platform==='windows') winEvents();
}
var DEMO=[{id:'nl',p:'vless',cc:'NL',name:'Netherlands',city:'Amsterdam',ping:24},
  {id:'de',p:'vless',cc:'DE',name:'Germany',city:'Frankfurt',ping:29},
  {id:'gb',p:'trojan',cc:'GB',name:'United Kingdom',city:'London',ping:33},
  {id:'us',p:'vless',cc:'US',name:'United States',city:'New York',ping:46},
  {id:'sg',p:'trojan',cc:'SG',name:'Singapore',city:'Singapore',ping:78},
  {id:'jp',p:'vless',cc:'JP',name:'Japan',city:'Tokyo',ping:97}];
boot();
})();
