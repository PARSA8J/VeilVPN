<div align="center">

# 🛡️ VeilVPN

**See nothing. Be invisible.**

A free, open-source VPN client for **Android & Windows** with a stunning **Liquid Glass** UI.
Real VPN core built-in — no v2rayNG, no extra apps, no clutter.

[![Release](https://img.shields.io/github/v/release/PARSA8J/VeilVPN?style=flat-square&color=6366f1)](https://github.com/PARSA8J/VeilVPN/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Android%20%7C%20Windows-blue?style=flat-square)](https://github.com/PARSA8J/VeilVPN/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/github/actions/workflow/status/PARSA8J/VeilVPN/build.yml?style=flat-square&label=CI)](https://github.com/PARSA8J/VeilVPN/actions)
[![Stars](https://img.shields.io/github/stars/PARSA8J/VeilVPN?style=flat-square&color=yellow)](https://github.com/PARSA8J/VeilVPN/stargazers)

**Download → [Latest Release](https://github.com/PARSA8J/VeilVPN/releases/latest)**

[Download](#-download) · [Features](#-features) · [Install](#-install) · [Security](#-security) · [Build](#-build-from-source) · [فارسی](#-فارسی)

</div>

---

## 📥 Download

| Platform | File | Requirement |
|---|---|---|
| 📱 Android | [VeilVPN-1.6.1.apk](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.1.apk) | Android 7.0+ (all CPUs) |
| 🖥️ Windows | [VeilVPN-1.6.1.exe](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.1.exe) | Windows 10/11 64-bit |

> 💡 If GitHub is blocked on your network, turn on any temporary proxy, or grab the files from the [Releases page](../../releases). The latest version always lives at `releases/latest`.

## ✨ Features

- **🪟 Liquid Glass UI** — a real glassmorphism design language: layered translucent panels, specular highlights, soft shadows. On Windows the window even has genuine acrylic blur behind it. No browser, no WebView on Windows — a fully **native Rust (egui)** app.
- **📡 Always-ready servers** — a curated list of live public servers (30+ countries) is baked into the app, plus live refresh from multiple public aggregator sources with CDN mirrors. The app **measures real ping to every server** (TCP handshake time) and shows the **country flag + country name + latency** for each one. By default you see only the **24 fastest** — tap *Show all* for the complete list.
- **📊 Real stats** — live download/upload speed from actual tunnel byte counters and true round-trip ping to your connected server. Nothing simulated.
- **🔁 Automatic failover** — if a server dies mid-session, up to 5 candidates are retried without you noticing.
- **🌍 16 languages** — English by default; switch anytime in Settings (full RTL for Persian, Arabic, Hebrew, Urdu).
- **🔐 Anti DNS-leak** — all DNS queries are resolved inside the encrypted tunnel; IPv6 is handled safely so nothing leaks outside.
- **⚙️ Custom subscription** — paste your own subscription link (V2Ray/Clash style) in Settings to use your private servers.
- **🎁 Free trial** — a 3-day trial is provisioned automatically on first launch. Premium keys via the Telegram bot [`@veilvpnverifybot`](https://t.me/veilvpnverifybot).

**Protocols:** VLESS (+ WebSocket / TLS), Trojan, Shadowsocks (AEAD ciphers)

## 📲 Install

### Android
1. Download the APK and open it (allow *"Install unknown apps"* for your browser).
2. If Play Protect shows a warning → **Install anyway** (the APK is signed with the project's public key; verify below).
3. Open VeilVPN → tap the big glass button. Done.

<details>
<summary><b>❓ "App not installed" — read this</b></summary>

- **v1.6.1 fixes installation on Android 11+** (older builds shipped a mis-packaged `resources.arsc`). Use the latest APK.
- If you still get it: you may have a **very old version (≤ 1.3.1)** installed with a different signature → **uninstall the old VeilVPN first**, then install the new APK.
- Make sure the downloaded file is ~136 KB and named `VeilVPN-1.6.1.apk` — a truncated download can also cause this.
- Verify the signature locally: the signing certificate SHA-256 is
  `8c1859dbf7a7f95bfe18ba228182ec73dee9ebb30a603c76987923d83ef7812d`.
</details>

### Windows
1. Download `VeilVPN-1.6.1.exe` and run it — no installer, no browser, no dependencies.
2. Tap connect; the **system proxy** is configured automatically, and removed cleanly on disconnect.

## 🔒 Security

- **100% open source** — every line of the VPN core, UI and build scripts is in this repo. GitHub Actions builds the official APK/EXE **from source, reproducibly** — nothing ships that isn't here.
- **Strong cryptography** — TLS 1.3 via rustls (Windows) / TLS 1.2+ (Android), Shadowsocks AEAD (AES-256-GCM / ChaCha20-Poly1305). No plaintext protocols, no fallback to insecure modes.
- **No telemetry, no logs, no analytics** — the app talks to VPN servers and public config aggregators only. IP lookup uses HTTPS endpoints only; cleartext HTTP is disabled in the Android manifest.
- **Single transparent signature** — the release keystore is committed so anyone can confirm that local builds, CI builds and published APKs are byte-for-byte signed by the same key (`SHA-256: 8c1859db…`).
- **Minimal permissions** — Android: `INTERNET`, `NETWORK_STATE`, `FOREGROUND_SERVICE`, `RECEIVE_BOOT_COMPLETED`. Nothing else. No contacts, no storage, no location.
- **Backups disabled** so tunnel credentials can't be extracted via `adb backup`.

## 🛠 Build from source

### Android
Only Java + Python needed — `setup_tools.sh` downloads the rest (aapt2, ecj, d8, apksigner).

```bash
bash scripts/setup_tools.sh
bash scripts/build_apk.sh     # -> VeilVPN-1.6.1.apk
```

### Windows (native Rust)
```bash
rustup target add x86_64-pc-windows-gnu
cd windows-rs && cargo test   # end-to-end protocol tests against simulated servers
cd .. && bash scripts/build_exe.sh   # -> VeilVPN-1.6.1.exe
```

### Server seed list
```bash
python3 scripts/fetch_seeds.py && python3 scripts/select_seeds.py
```

## 📁 Repository layout

```
shared/       Liquid Glass web UI (Android) + i18n for 16 languages
android/      Android app: VpnService + user-space TCP/IP stack + VLESS/Trojan/SS clients
windows-rs/   Native Windows app (Rust + egui, acrylic glass theme) + e2e tests
scripts/      Build scripts (APK / EXE / seeds / icon)
.github/      CI: builds both binaries on every push, releases on every v* tag
```

CI builds the APK and EXE on every push (see the **Actions** tab); every `v*` tag is published automatically as a GitHub Release.

## 📄 License

MIT — completely free, use it however you like.

---

<div dir="rtl">

## 🛡️ فارسی

**VeilVPN** یک کلاینت VPN متن‌باز و رایگان برای **اندروید و ویندوز** با تم حرفه‌ای **Liquid Glass** است — هستهٔ VPN واقعی داخل خود برنامه، بدون نیاز به هیچ اپ دیگری.

- 📱 **اندروید** (`VeilVPN-1.6.1.apk`) — VpnService بومی + استک TCP/IP یوزر-اسپیس + UI شیشه‌ای؛ بدون v2rayNG و برنامهٔ جانبی.
- 🖥️ **ویندوز** (`VeilVPN-1.6.1.exe`) — اپ کاملاً **نیتیو Rust** (بدون مرورگر!) با بلور واقعی Acrylic + هستهٔ پروکسی داخل خود exe + ست‌شدن خودکار پروکسی سیستم.
- 📡 **سرورها با پرچم، اسم کشور و پینگ واقعی** — لیست از قبل داخل برنامه است + به‌روزرسانی زنده؛ پینگ هر سرور واقعاً اندازه‌گیری می‌شود و به‌صورت پیش‌فرض **۲۴ سرور سریع‌تر** را می‌بینید.
- 📊 **آمار واقعی** — سرعت و پینگ از شمارنده‌های واقعی تونل، نه عدد تقلبی.
- 🔁 Failover خودکار · 🔒 ضد DNS leak · 🌍 ۱۶ زبان (پیش‌فرض انگلیسی) · ⚙️ سابسکریپشن دلخواه
- 🔓 **۳ روز آزمایش رایگان خودکار**؛ کلید پریمیوم از ربات [`@veilvpnverifybot`](https://t.me/veilvpnverifybot)

**پروتکل‌ها:** VLESS (+WS/TLS)، Trojan، Shadowsocks (AES-GCM)

### 📥 دانلود

| پلتفرم | فایل | نیاز |
|---|---|---|
| 📱 اندروید | [VeilVPN-1.6.1.apk](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.1.apk) | اندروید ۷+ |
| 🖥️ ویندوز | [VeilVPN-1.6.1.exe](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.1.exe) | ویندوز ۱۰/۱۱ ۶۴ بیتی |

### ❓ «App not installed» — عیب‌یابی

- **نسخهٔ ۱.۶.۱ مشکل نصب روی اندروید ۱۱ به بالا را کاملاً فیکس کرده** (بسته‌بندی اشتباه `resources.arsc` در نسخه‌های قدیمی). حتماً آخرین APK را بگیرید.
- اگر باز هم نشد: نسخهٔ خیلی قدیمی (≤ ۱.۳.۱) با امضای متفاوت نصب دارید → اول **VeilVPN قبلی را پاک کنید**، بعد APK جدید را نصب کنید.
- اگر Play Protect هشدار داد → «نصب همان‌جا / Install anyway». اپ متن‌باز است و امضایش در بالا قابل راستی‌آزمایی است.

### 📄 مجوز

MIT — کامل آزاد.

</div>
