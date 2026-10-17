#!/usr/bin/env python3
"""Select a diverse, alive seed subset with parallel TCP pings."""
import json, re, socket, sys, base64, urllib.parse, os
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, '..', 'shared', 'seed.txt')
MAXN = 420

def b64d(s):
    s = s.strip().replace('-', '+').replace('_', '/'); s += '=' * (-len(s) % 4)
    try: return base64.b64decode(s).decode('utf-8', 'replace')
    except Exception: return None

FLAGS = ''.join(chr(c) for c in range(0x1F1E6, 0x1F200))
def flag_to_cc(f):
    if len(f) == 2 and all(0x1F1E6 <= ord(c) <= 0x1F1FF for c in f):
        return ''.join(chr(ord(c) - 0x1F1E6 + 65) for c in f)
    return None

def probe(u):
    try:
        if u.startswith('vless://') or u.startswith('trojan://'):
            proto = u.split(':', 1)[0]
            body = u[len(proto) + 3:]
            frag = body.split('#', 1)
            remark = urllib.parse.unquote(frag[1]) if len(frag) > 1 else ''
            main = frag[0].split('?', 1)[0]
            qs = urllib.parse.parse_qs(frag[0].split('?', 1)[1]) if '?' in frag[0] else {}
            if qs.get('type', [''])[0] in ('grpc', 'kcp', 'quic', 'httpupgrade'):
                return None
            if 'grpc' in qs.get('path', [''])[0] or qs.get('security', [''])[0] == 'reality':
                return None
            if qs.get('flow', [''])[0] == 'xtls-rprx-vision':
                return None
            userinfo, hostport = main.rsplit('@', 1)
            if '@' in userinfo: return None
            if hostport.startswith('['):
                host = hostport.split(']')[0][1:]; port = int(hostport.split(']:')[1])
            else:
                host, port = hostport.rsplit(':', 1); port = int(port)
            m = re.search('[' + FLAGS + ']{2}', remark)
            cc = flag_to_cc(m.group(0)) if m else None
            if not cc:
                m2 = re.search(r'\b([A-Z]{2})\b', remark.upper())
                cc = m2.group(1) if m2 else None
            return dict(p=proto, h=host, pt=port, cc=cc, r=remark[:60], u=u)
        if u.startswith('ss://'):
            body = u[5:].split('#', 1)
            remark = urllib.parse.unquote(body[1]) if len(body) > 1 else ''
            main = body[0].split('/?plugin=', 1)[0].rstrip('/')
            if '@' not in main:
                dec = b64d(main)
                if not dec or '@' not in dec: return None
                main = dec
            userinfo, hostport = main.rsplit('@', 1)
            if hostport.startswith('['):
                host = hostport.split(']')[0][1:]; port = int(hostport.split(']:')[1])
            else:
                host, port = hostport.rsplit(':', 1); port = int(port)
            if b64d(userinfo): userinfo = b64d(userinfo)
            method = urllib.parse.unquote(userinfo).split(':', 1)[0]
            if 'gcm' not in method and 'chacha' not in method: return None
            m = re.search('[' + FLAGS + ']{2}', remark)
            cc = flag_to_cc(m.group(0)) if m else None
            if not cc:
                m2 = re.search(r'\b([A-Z]{2})\b', remark.upper())
                cc = m2.group(1) if m2 else None
            return dict(p='ss', h=host, pt=port, cc=cc, r=remark[:60], u=u)
        if u.startswith('vmess://'):
            dec = b64d(u[8:])
            if not dec: return None
            j = json.loads(dec)
            if str(j.get('net')) in ('grpc', 'kcp', 'quic'): return None
            if str(j.get('tls')) == 'reality': return None
            remark = str(j.get('ps', ''))[:60]
            m = re.search('[' + FLAGS + ']{2}', remark)
            cc = flag_to_cc(m.group(0)) if m else None
            if not cc:
                m2 = re.search(r'\b([A-Z]{2})\b', remark.upper())
                cc = m2.group(1) if m2 else None
            return dict(p='vmess', h=str(j.get('add')), pt=int(j.get('port')), cc=cc, r=remark, u=u)
    except Exception:
        return None
    return None

def tcp_ping(h, pt):
    try:
        s = socket.create_connection((h, pt), timeout=2.0)
        s.close()
        return True
    except Exception:
        return False

def main():
    uris = [l.strip() for l in open(SRC) if l.strip()]
    print(f"loaded {len(uris)} uris", file=sys.stderr)
    with ThreadPoolExecutor(64) as ex:
        nodes = [n for n in ex.map(probe, uris) if n]
    print(f"parsed {len(nodes)}", file=sys.stderr)
    seen, uniq = set(), []
    for n in nodes:
        k = (n['h'], n['pt'])
        if k in seen: continue
        seen.add(k); uniq.append(n)
    with ThreadPoolExecutor(256) as ex:
        alive = [n for n, ok in zip(uniq, ex.map(lambda n: tcp_ping(n['h'], n['pt']), uniq)) if ok]
    print(f"alive {len(alive)}", file=sys.stderr)
    groups = {}
    for n in alive:
        cc = n['cc'] or 'XX'
        groups.setdefault(cc, []).append(n)
    for g in groups.values():
        g.sort(key=lambda n: 0 if n['p'] in ('vless', 'trojan') else 1)
    picked, i = [], 0
    ccs = sorted(groups.keys(), key=lambda c: -len(groups[c]))
    while len(picked) < MAXN and any(groups[c] for c in ccs):
        for c in ccs:
            if groups[c] and i < len(groups[c]):
                picked.append(groups[c][i]); groups[c].remove(groups[c][i])
                if len(picked) >= MAXN: break
        i += 1
        if i > 400: break
    # drop vmess (unsupported in 1.5.1 builds)
    picked = [n for n in picked if n['p'] != 'vmess']
    with open(SRC, 'w') as f:
        f.write('\n'.join(n['u'] for n in picked))
    ccsum, pron = {}, {}
    for n in picked:
        c = n['cc'] or 'XX'; ccsum[c] = ccsum.get(c, 0) + 1; pron[n['p']] = pron.get(n['p'], 0) + 1
    print(json.dumps({'picked': len(picked), 'countries': len(ccsum), 'proto': pron}))

if __name__ == '__main__':
    main()
