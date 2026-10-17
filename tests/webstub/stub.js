/* v1.5.1 bridge stub for headless UI testing (mimics MainActivity Bridge + pushes) */
(function(){
  'use strict';
  var trialEnd = Date.now() + 3*24*3600*1000;
  var lic = {type:'free', exp:trialEnd, daysLeft:3, active:true, device:'deadbeef'};
  var st = {status:'disconnected', server:null, features:{kill:true,auto:true,adblock:false}};
  var SEED = [
    {id:'a1',p:'vless',cc:'DE',name:'Germany-1',ping:24},{id:'a2',p:'trojan',cc:'NL',name:'NL-1',ping:31},
    {id:'a3',p:'vless',cc:'US',name:'US-1',ping:88},{id:'a4',p:'ss',cc:'TR',name:'TR-1'},{id:'a5',p:'vless',cc:'SG',name:'SG-1',ping:130}];
  window.VeilNative = {
    api: function(action, payloadJson, cbId){
      setTimeout(function(){
        try {
          var p = JSON.parse(payloadJson||'{}');
          var R = function(o){ window.__nCb(cbId, JSON.stringify(o)); };
          switch(action){
            case 'fetchServers': R({servers: SEED}); break;
            case 'pingBatch': R({pings:{a4:45}}); break;
            case 'connect':
              if(!lic.active){ R({ok:false,error:'activate'}); break; }
              R({ok:true, async:true, server:{id:p.id||'a1',cc:'DE',name:'Germany-1'}});
              st.status='connecting'; pushStatus();
              setTimeout(function(){ st.status='connected'; st.server={id:'a1',cc:'DE',name:'Germany-1'}; pushStatus(); }, 900);
              break;
            case 'disconnect': st.status='disconnected'; st.server=null; R({ok:true}); pushStatus(); break;
            case 'getState': R({status:st.status, platform:'android', version:'1.5.1', features:st.features, lic:lic}); break;
            case 'licenseInfo': R(lic); break;
            case 'activateKey':
              if((p.key||'').startsWith('VEIL-')){ lic={type:'pre',exp:0,daysLeft:9999,active:true,device:'deadbeef'}; R({ok:true,lic:lic}); }
              else R({ok:false,error:'invalid key'});
              break;
            case 'deactivateKey': lic.active=false; R({ok:true}); break;
            case 'setFeature': st.features[p.key]=p.on; R({ok:true}); break;
            case 'setLang': R({ok:true}); break;
            case 'setSource': R({ok:true}); break;
            case 'getIpInfo': R({ip:'185.12.34.56', cc:'DE', city:'Frankfurt'}); break;
            case 'copyText': R({ok:true}); break;
            case 'openUrl': R({ok:true}); break;
            default: window.__nErr(cbId, 'unknown '+action);
          }
        } catch(e){ window.__nErr(cbId, String(e)); }
      }, 60);
    }
  };
  function pushStatus(){ setTimeout(function(){ window.__nEvent && __nEvent('status', JSON.stringify({state:st.status, server:st.server, error:null})); }, 30); }
})();
