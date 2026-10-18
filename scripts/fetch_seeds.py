#!/usr/bin/env python3
"""Fetch free proxy configs from public aggregators, parse, dedupe, build seed file."""
import base64, json, re, sys, urllib.request, urllib.parse, ssl

UA = {'User-Agent': 'Mozilla/5.0 (Linux; Android 13) VeilVPN/1.5.1'}
ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

SOURCES = [
    ("https://raw.githubusercontent.com/Epodonios/v2ray-configs/main/All_Configs_Sub.txt",
     "https://cdn.jsdelivr.net/gh/Epodonios/v2ray-configs@main/All_Configs_Sub.txt"),
    ("https://raw.githubusercontent.com/mahdibland/V2RayAggregator/master/sub/sub_merge.txt",
     "https://cdn.jsdelivr.net/gh/mahdibland/V2RayAggregator@master/sub/sub_merge.txt"),
    ("https://raw.githubusercontent.com/barry-far/V2ray-Configs/main/All_Configs_Sub.txt",
     "https://cdn.jsdelivr.net/gh/barry-far/V2ray-Configs@main/All_Configs_Sub.txt"),
]

def fetch(url):
    try:
        req = urllib.request.Request(url, headers=UA)
        with urllib.request.urlopen(req, timeout=20, context=ctx) as r:
            return r.read().decode('utf-8', 'replace')
    except Exception as e:
        print(f"  FAIL {url[:80]} : {e}", file=sys.stderr)
        return None

def b64d(s):
    s = s.strip().replace('-', '+').replace('_', '/')
    s += '=' * (-len(s) % 4)
    try:
        return base64.b64decode(s).decode('utf-8', 'replace')
    except Exception:
        return None

def clean_uri(uri):
    uri = uri.strip()
    if not uri or uri.startswith('//') or uri.startswith('#'):
        return None
    if not re.match(r'^(vless|trojan|vmess|ss)://', uri):
        return None
    if uri.startswith('vmess://'):
        dec = b64d(uri[8:])
        if not dec:
            return None
        try:
            j = json.loads(dec)
        except Exception:
            return None
        if str(j.get('net', '')) in ('grpc', 'kcp', 'quic'):
            return None
        return uri
    return uri

def main():
    uris = {}
    stats = {'proto': {}, 'src': 0}
    for plain, mirror in SOURCES:
        txt = fetch(plain) or fetch(mirror)
        if not txt:
            continue
        stats['src'] += 1
        lines = []
        for ln in txt.splitlines():
            ln = ln.strip()
            if len(ln) > 200 and '://' not in ln:
                dec = b64d(ln)
                if dec:
                    lines.extend(x.strip() for x in dec.splitlines())
            else:
                lines.append(ln)
        n = 0
        for ln in lines:
            u = clean_uri(ln)
            if u:
                key = u.split('#')[0][:120]
                if key not in uris:
                    uris[key] = u
                    p = u.split(':', 1)[0]
                    stats['proto'][p] = stats['proto'].get(p, 0) + 1
                    n += 1
        print(f"  +{n} from {plain.split('/')[2]}/{plain.split('/')[3]}", file=sys.stderr)
    out = list(uris.values())
    out.sort(key=lambda u: 0 if u.startswith(('vless', 'trojan', 'ss')) else 1)
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, '..', 'shared', 'seed.txt'), 'w') as f:
        f.write('\n'.join(out))
    print(json.dumps({'total': len(out), 'sources': stats['src'], 'proto': stats['proto']}))

import os
if __name__ == '__main__':
    main()
