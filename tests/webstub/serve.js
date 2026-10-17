// Serves shared/index.html with stub.js injected, for headless UI testing
const http = require('http'), fs = require('fs'), path = require('path');
const ROOT = path.resolve(__dirname, '../../shared');
const MIME = {'.html':'text/html','.js':'text/javascript','.css':'text/css','.txt':'text/plain'};
http.createServer((req,res)=>{
  let p = req.url.split('?')[0];
  if (p === '/' || p === '/index.html') {
    let html = fs.readFileSync(path.join(ROOT,'index.html'),'utf8');
    html = html.replace('<script src="i18n.js">', '<script src="/stub.js"></script>\n<script src="i18n.js">');
    res.writeHead(200,{'Content-Type':'text/html'}); res.end(html); return;
  }
  if (p === '/stub.js') { res.writeHead(200,{'Content-Type':'text/javascript'}); res.end(fs.readFileSync(__dirname+'/stub.js')); return; }
  let f = path.join(ROOT, p);
  if (fs.existsSync(f)) { res.writeHead(200,{'Content-Type':MIME[path.extname(f)]||'application/octet-stream'}); res.end(fs.readFileSync(f)); }
  else { res.writeHead(404); res.end('no'); }
}).listen(8899, ()=>console.log('stub server :8899'));
