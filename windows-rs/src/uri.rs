//! URI parser for vless:// trojan:// ss:// links (same semantics as the Android Node.java).

use base64::Engine;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Server {
    pub proto: String,   // vless | trojan | ss
    pub host: String,
    pub port: u16,
    pub remark: String,
    // vless / trojan
    pub uuid: String,    // trojan: password
    pub security: String, // none | tls
    pub sni: String,
    pub allow_insecure: bool,
    pub flow: String,    // vless flow (xtls-rprx-vision…) — empty = plain
    pub net: String,     // tcp | ws
    pub ws_path: String,
    pub ws_host: String,
    // ss
    pub method: String,
    pub password: String,
    // meta
    pub cc: String,      // country code from remark flag
}

impl Server {
    pub fn key(&self) -> String {
        format!("{}|{}|{}|{}", self.proto, self.host, self.port, self.remark)
    }
}

fn b64d(s: &str) -> Option<Vec<u8>> {
    let t: String = s.trim().trim_end_matches('=').to_string();
    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(v) = engine.decode(&t) {
            return Some(v);
        }
    }
    None
}

fn b64d_str(s: &str) -> String {
    b64d(s).and_then(|v| String::from_utf8(v).ok()).unwrap_or_default()
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hx = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hx {
                out.push(v);
                i += 3;
                continue;
            }
        }
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extract ISO country code from remark flags like 🇩🇪 / [DE] / DE-01 / (US)
pub fn cc_of_remark(remark: &str) -> String {
    // flag emoji = two regional indicator symbols
    let chars: Vec<char> = remark.chars().collect();
    for w in chars.windows(2) {
        let a = w[0] as u32;
        let b = w[1] as u32;
        if (0x1F1E6..=0x1F1FF).contains(&a) && (0x1F1E6..=0x1F1FF).contains(&b) {
            let mut s = String::new();
            s.push(((a - 0x1F1E6) as u8 + b'A') as char);
            s.push(((b - 0x1F1E6) as u8 + b'A') as char);
            return s;
        }
    }
    // textual: [US] (US) US- US_ US| |US| "US" —- look for a standalone 2-letter code
    let up = remark.to_uppercase();
    let b = up.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        let c = b[i] as char;
        let d = b[i + 1] as char;
        let ok_c = c.is_ascii_alphabetic();
        let ok_d = d.is_ascii_alphabetic();
        if ok_c && ok_d {
            let before_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
            let after_ok = i + 2 >= b.len() || !b[i + 2].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return up[i..i + 2].to_string();
            }
        }
        i += 1;
    }
    String::new()
}

fn kv_of_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (url_decode(k), url_decode(v)),
            None => (url_decode(p), String::new()),
        })
        .collect()
}

fn parse_fragment_and_query(rest: &str) -> (String, String, String) {
    // rest = userinfo@host:port/path?query#fragment
    let (body, fragment) = match rest.split_once('#') {
        Some((a, b)) => (a, url_decode(b)),
        None => (rest, String::new()),
    };
    let (body, query) = match body.split_once('?') {
        Some((a, b)) => (a, b.to_string()),
        None => (body, String::new()),
    };
    (body.to_string(), query, fragment)
}

fn split_host_port(s: &str) -> Option<(String, u16)> {
    let s = s.trim();
    if s.starts_with('[') {
        // [ipv6]:port
        let close = s.find(']')?;
        let host = &s[1..close];
        let port = s[close + 1..].trim_start_matches(':').parse().ok()?;
        return Some((host.to_string(), port));
    }
    let idx = s.rfind(':')?;
    let host = &s[..idx];
    let port = s[idx + 1..].parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port))
}

fn blank() -> Server {
    Server { security: "none".into(), net: "tcp".into(), ..Default::default() }
}

pub fn parse_uri(uri: &str) -> Option<Server> {
    let uri = uri.trim();
    let (scheme, rest) = uri.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let (body, query, fragment) = parse_fragment_and_query(rest);
    let q = kv_of_query(&query);

    let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone()).unwrap_or_default();

    match scheme.as_str() {
        "vless" => {
            let (userinfo, hostport) = body.rsplit_once('@')?;
            let (host, port) = split_host_port(hostport)?;
            let mut s = blank();
            s.proto = "vless".into();
            s.host = host;
            s.port = port;
            s.uuid = url_decode(userinfo);
            let security = get("security");
            s.security = if security.is_empty() { "none".into() } else { security };
            s.sni = get("sni");
            if s.sni.is_empty() {
                s.sni = get("host");
            }
            s.allow_insecure = matches!(get("allowInsecure").as_str(), "1" | "true");
            s.flow = get("flow");
            let n = get("type");
            s.net = if n.is_empty() { "tcp".into() } else { n };
            s.ws_path = get("path");
            s.ws_host = get("host");
            if s.ws_path.is_empty() && s.net == "ws" {
                // path may appear before '?'
                if let Some((_, p)) = body.split_once('/') {
                    s.ws_path = format!("/{}", p);
                }
            }
            s.remark = if fragment.is_empty() { s.host.clone() } else { fragment };
            s.cc = cc_of_remark(&s.remark);
            Some(s)
        }
        "trojan" => {
            let (userinfo, hostport) = body.rsplit_once('@')?;
            let (host, port) = split_host_port(hostport)?;
            let mut s = blank();
            s.proto = "trojan".into();
            s.host = host;
            s.port = port;
            s.uuid = url_decode(userinfo); // trojan password
            let security = get("security");
            s.security = if security.is_empty() { "tls".into() } else { security };
            s.sni = get("sni");
            if s.sni.is_empty() {
                s.sni = get("peer");
            }
            s.allow_insecure = matches!(get("allowInsecure").as_str(), "1" | "true");
            let n = get("type");
            s.net = if n.is_empty() { "tcp".into() } else { n };
            s.ws_path = get("path");
            s.ws_host = get("host");
            s.remark = if fragment.is_empty() { s.host.clone() } else { fragment };
            s.cc = cc_of_remark(&s.remark);
            Some(s)
        }
        "ss" => {
            // forms: ss://base64(method:pass@host:port)#remark
            //        ss://base64(method:pass)@host:port?params#remark
            //        ss://method:pass@host:port (plain, rare)
            let mut s = blank();
            s.proto = "ss".into();
            s.security = "tls".into();
            let (userinfo, hostport) = match body.rsplit_once('@') {
                Some((u, h)) => {
                    if u.contains(':') && !b64d(u).is_some() {
                        (u.to_string(), h.to_string())
                    } else if let Some(decoded) = Some(b64d_str(u)) {
                        if decoded.contains('@') && decoded.contains(':') {
                            let (m, hh) = decoded.rsplit_once('@')?;
                            (m.to_string(), hh.to_string())
                        } else {
                            (u.to_string(), h.to_string())
                        }
                    } else {
                        (u.to_string(), h.to_string())
                    }
                }
                None => {
                    // whole body base64
                    let dec = b64d_str(&body);
                    let (m, hh) = dec.rsplit_once('@')?;
                    (m.to_string(), hh.to_string())
                }
            };
            let (host, port) = split_host_port(&hostport)?;
            s.host = host;
            s.port = port;
            if let Some((m, p)) = userinfo.split_once(':') {
                // method may be base64 itself
                if let Some(dec) = b64d(m) {
                    if let Ok(dm) = String::from_utf8(dec) {
                        if dm.contains('-') {
                            s.method = dm;
                            s.password = url_decode(p);
                        }
                    }
                }
                if s.method.is_empty() {
                    s.method = m.to_string();
                    s.password = url_decode(p);
                }
            } else {
                let dec = b64d_str(&userinfo);
                let (m, p) = dec.split_once(':')?;
                s.method = m.to_string();
                s.password = p.to_string();
            }
            if !s.method.contains("-gcm") && !s.method.contains("chacha") && !s.method.contains("aes-") {
                // maybe whole userinfo was method:pass base64-in-base64; bail if unsupported method
                if !s.method.contains("gcm") {
                    return None;
                }
            }
            s.remark = if fragment.is_empty() { s.host.clone() } else { fragment };
            s.cc = cc_of_remark(&s.remark);
            Some(s)
        }
        _ => None,
    }
}

pub fn parse_lines(text: &str) -> Vec<Server> {
    let mut out = Vec::new();
    let mut txt = String::new();
    let trimmed = text.trim();
    // if the whole payload looks like base64 blob, decode it
    if !trimmed.contains("://") {
        if let Some(dec) = b64d(trimmed) {
            if let Ok(s) = String::from_utf8(dec) {
                txt = s;
            }
        }
    }
    if txt.is_empty() {
        txt = text.to_string();
    }
    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if !line.contains("://") {
            if let Some(dec) = b64d(line) {
                if let Ok(s) = String::from_utf8(dec) {
                    for l2 in s.lines() {
                        if let Some(sv) = parse_uri(l2) {
                            out.push(sv);
                        }
                    }
                    continue;
                }
            }
            continue;
        }
        if let Some(sv) = parse_uri(line) {
            out.push(sv);
        }
    }
    out
}

pub fn dedupe(servers: Vec<Server>) -> Vec<Server> {
    let mut seen = std::collections::HashSet::new();
    servers.into_iter().filter(|s| seen.insert(s.key())).collect()
}

/// Supported methods on this build
pub fn method_supported(m: &str) -> bool {
    matches!(
        m,
        "aes-128-gcm" | "aes-256-gcm" | "chacha20-ietf-poly1305" | "chacha20-poly1305"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vless_tcp_tls() {
        let s = parse_uri("vless://a7ba5f2c-0e4d-4a4f-9d1c-3f2b6c7d8e9f@1.2.3.4:443?encryption=none&security=tls&sni=ex.com&type=tcp#%F0%9F%87%A9%F0%9F%87%AA%20Berlin%201").unwrap();
        assert_eq!(s.proto, "vless");
        assert_eq!(s.host, "1.2.3.4");
        assert_eq!(s.port, 443);
        assert_eq!(s.security, "tls");
        assert_eq!(s.sni, "ex.com");
        assert_eq!(s.net, "tcp");
        assert_eq!(s.remark, "🇩🇪 Berlin 1");
        assert_eq!(s.cc, "DE");
    }

    #[test]
    fn test_vless_ws() {
        let s = parse_uri("vless://uuid@ex.com:80?path=%2Fws&host=cdn.com&type=ws&security=none#WS-1").unwrap();
        assert_eq!(s.net, "ws");
        assert_eq!(s.ws_path, "/ws");
        assert_eq!(s.ws_host, "cdn.com");
    }

    #[test]
    fn test_trojan() {
        let s = parse_uri("trojan://passw0rd@tj.com:443?sni=tj.com&allowInsecure=1#TJ").unwrap();
        assert_eq!(s.proto, "trojan");
        assert_eq!(s.uuid, "passw0rd");
        assert_eq!(s.security, "tls");
        assert!(s.allow_insecure);
    }

    #[test]
    fn test_ss_plain_userinfo() {
        let s = parse_uri("ss://YWVzLTI1Ni1nY206c2VjcmV0QDEuMi4zLjQ6NDQzNA==#SS1").unwrap();
        assert_eq!(s.method, "aes-256-gcm");
        assert_eq!(s.password, "secret");
        assert_eq!(s.host, "1.2.3.4");
        assert_eq!(s.port, 4434);
    }

    #[test]
    fn test_lines_blob() {
        let blob = base64::engine::general_purpose::STANDARD
            .encode("vless://u@h1.com:443?security=tls#A\ntrojan://p@h2.com:443#B");
        let v = parse_lines(&blob);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].host, "h1.com");
        assert_eq!(v[1].proto, "trojan");
    }

    #[test]
    fn test_cc() {
        assert_eq!(cc_of_remark("[US] New York"), "US");
        assert_eq!(cc_of_remark("🇯🇵 Tokyo 01"), "JP");
        assert_eq!(cc_of_remark("DE-01 Fast"), "DE");
    }
}
