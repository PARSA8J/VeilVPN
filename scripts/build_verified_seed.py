#!/usr/bin/env python3
"""Build Mehrveil verified seed + Pages sub from live aggregator sources.

Mirror of the Windows engine's source groups + the app's exact transport
filters. Every node is TCP-alive-tested (real data — no fake nodes).
Writes:
  shared/seed.txt            (cap SEED_N=150)  -> embedded in APK
  windows-rs/assets/seed.txt (same content)    -> embedded in exe
  docs/sub.txt               (cap SUB_N=400)   -> served on GitHub Pages
"""
import base64, json, os, re, socket, ssl, sys, time, urllib.request, urllib.parse
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SEED_N = 150
SUB_N = 400
TCP_TIMEOUT = 3.0

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE
UA = {"User-Agent": "veilvpn/1.6", "Accept": "*/*"}

# same groups as windows-rs/src/engine.rs SOURCE_GROUPS (v1.6.12)
GROUPS = [
    ["https://parsa8j.github.io/VeilVPN/sub.txt"],
    [
        "https://gh-proxy.com/https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
        "https://ghproxy.net/https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
        "https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
        "https://gcore.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
        "https://fastly.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
        "https://testingcf.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt",
        "https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
    ],
    [
        "https://cdn.jsdelivr.net/gh/mahdibland/V2RayAggregator@master/sub/sub_merge.txt",
        "https://raw.githubusercontent.com/mahdibland/V2RayAggregator/master/sub/sub_merge.txt",
    ],
]

def fetch(url, timeout=12, limit=12_000_000):
    try:
        req = urllib.request.Request(url, headers=UA)
        with urllib.request.urlopen(req, timeout=timeout, context=ctx) as r:
            return r.read()[:limit]
    except Exception:
        return None

def b64d(s):
    s = s.strip().replace('-', '+').replace('_', '/')
    s += '=' * (-len(s) % 4)
    try:
        return base64.b64decode(s).decode('utf-8', 'replace')
    except Exception:
        return None

def decode_sub(txt):
    txt = txt.strip()
    if '://' in txt:
        return txt
    dec = b64d(txt)
    return dec if dec and '://' in dec else txt

def fetch_group(urls):
    """parallel race, first usable body wins (same as the engine)."""
    with ThreadPoolExecutor(len(urls)) as ex:
        for body in ex.map(lambda u: fetch(u), urls):
            if body and b'://' in body:
                return decode_sub(body.decode('utf-8', 'replace'))
    return ''

FLAGS = ''.join(chr(c) for c in range(0x1F1E6, 0x1F200))
def flag_to_cc(f):
    if len(f) == 2 and all(0x1F1E6 <= ord(c) <= 0x1F1FF for c in f):
        return ''.join(chr(ord(c) - 0x1F1E6 + 65) for c in f)
    return None

def parse_line(uri):
    """App-exact filter: vless/trojan (security none|tls, net tcp|ws, no vision
    flow), ss aes-gcm only. Returns (proto, host, port, cc) or None."""
    try:
        uri = uri.strip()
        if uri.startswith('vless://') or uri.startswith('trojan://'):
            proto, body = uri.split(':', 1)
            body = body[2:]
            frag = body.split('#', 1)
            remark = urllib.parse.unquote(frag[1]) if len(frag) > 1 else ''
            main = frag[0].split('?', 1)[0]
            qs = urllib.parse.parse_qs(frag[0].split('?', 1)[1]) if '?' in frag[0] else {}
            net = qs.get('type', ['tcp'])[0]
            sec = qs.get('security', ['none' if proto == 'vless' else 'tls'])[0]
            if net not in ('tcp', 'ws', ''):
                return None
            if sec not in ('tls', 'none', ''):
                return None
            if qs.get('flow', [''])[0] == 'xtls-rprx-vision':
                return None
            userinfo, hostport = main.rsplit('@', 1)
            if '@' in userinfo:
                return None
            if hostport.startswith('['):
                host = hostport.split(']')[0][1:]
                port = int(hostport.split(']:')[1])
            else:
                host, port = hostport.rsplit(':', 1)
                port = int(port)
            cc = flag_to_cc(m.group(0)) if (m := re.search('[' + FLAGS + ']{2}', remark)) else None
            if not cc:
                m2 = re.search(r'\b([A-Z]{2})\b', remark.upper())
                cc = m2.group(1) if m2 else None
            return dict(p=proto, h=host, pt=port, cc=cc, u=uri)
        if uri.startswith('ss://'):
            body = uri[5:].split('#', 1)
            remark = urllib.parse.unquote(body[1]) if len(body) > 1 else ''
            main = body[0].split('/?plugin=', 1)[0].rstrip('/')
            if '@' not in main:
                dec = b64d(main)
                if not dec or '@' not in dec:
                    return None
                main = dec
            userinfo, hostport = main.rsplit('@', 1)
            if hostport.startswith('['):
                host = hostport.split(']')[0][1:]
                port = int(hostport.split(']:')[1])
            else:
                host, port = hostport.rsplit(':', 1)
                port = int(port)
            if b64d(userinfo):
                userinfo = b64d(userinfo)
            method = urllib.parse.unquote(userinfo).split(':', 1)[0]
            if method not in ('aes-128-gcm', 'aes-256-gcm'):
                return None
            cc = flag_to_cc(m.group(0)) if (m := re.search('[' + FLAGS + ']{2}', remark)) else None
            if not cc:
                m2 = re.search(r'\b([A-Z]{2})\b', remark.upper())
                cc = m2.group(1) if m2 else None
            return dict(p='ss', h=host, pt=port, cc=cc, u=uri)
    except Exception:
        return None
    return None

def tcp_alive(h, pt):
    try:
        s = socket.create_connection((h, pt), timeout=TCP_TIMEOUT)
        s.close()
        return True
    except Exception:
        return False

def main():
    t0 = time.time()
    texts = []
    with ThreadPoolExecutor(len(GROUPS)) as ex:
        texts = list(ex.map(fetch_group, GROUPS))
    for i, t in enumerate(texts):
        print(f"group {i}: {len(t)} bytes", file=sys.stderr)
    raw_lines = '\n'.join(texts).splitlines()
    # also split base64 blobs embedded per-line
    uris = []
    for ln in raw_lines:
        ln = ln.strip()
        if not ln:
            continue
        if len(ln) > 200 and '://' not in ln:
            dec = b64d(ln)
            if dec:
                uris.extend(x.strip() for x in dec.splitlines() if x.strip())
        else:
            uris.append(ln)
    print(f"candidate lines: {len(uris)}", file=sys.stderr)

    seen, nodes = set(), []
    with ThreadPoolExecutor(64) as ex:
        parsed = [n for n in ex.map(parse_line, uris) if n]
    for n in parsed:
        k = (n['h'], n['pt'])
        if k in seen:
            continue
        seen.add(k)
        nodes.append(n)
    print(f"parsed+deduped: {len(nodes)}", file=sys.stderr)

    with ThreadPoolExecutor(256) as ex:
        alive_ok = list(ex.map(lambda n: tcp_alive(n['h'], n['pt']), nodes))
    alive = [n for n, ok in zip(nodes, alive_ok) if ok]
    print(f"TCP-alive: {len(alive)}  ({time.time()-t0:.0f}s)", file=sys.stderr)

    # country-diverse round-robin (same policy as select_seeds.py)
    groups = {}
    for n in alive:
        groups.setdefault(n['cc'] or 'XX', []).append(n)
    for g in groups.values():
        g.sort(key=lambda n: (0 if n['p'] in ('vless', 'trojan') else 1, n['h']))
    ccs = sorted(groups.keys(), key=lambda c: -len(groups[c]))
    picked, i = [], 0
    while len(picked) < SUB_N and any(groups[c] for c in ccs):
        for c in ccs:
            if groups[c] and i < len(groups[c]):
                picked.append(groups[c][i])
                groups[c].remove(groups[c][i])
                if len(picked) >= SUB_N:
                    break
        i += 1
        if i > SUB_N:
            break

    seed_lines = [n['u'] for n in picked[:SEED_N]]
    sub_lines = [n['u'] for n in picked]
    with open(os.path.join(ROOT, 'shared', 'seed.txt'), 'w') as f:
        f.write('\n'.join(seed_lines) + '\n')
    with open(os.path.join(ROOT, 'windows-rs', 'assets', 'seed.txt'), 'w') as f:
        f.write('\n'.join(seed_lines) + '\n')
    with open(os.path.join(ROOT, 'docs', 'sub.txt'), 'w') as f:
        f.write('\n'.join(sub_lines) + '\n')

    ccsum, pron = {}, {}
    for n in picked:
        c = n['cc'] or 'XX'
        ccsum[c] = ccsum.get(c, 0) + 1
        pron[n['p']] = pron.get(n['p'], 0) + 1
    print(json.dumps({
        'sub': len(sub_lines), 'seed': len(seed_lines),
        'countries': len(ccsum), 'top_cc': dict(sorted(ccsum.items(), key=lambda kv: -kv[1])[:10]),
        'proto': pron, 'secs': round(time.time() - t0, 1),
    }, ensure_ascii=False))

if __name__ == '__main__':
    main()
