<div align="center">

# 🛡️ Mehrveil (formerly VeilVPN)

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
| 📱 Android | [Mehrveil-1.6.2.apk](https://github.com/PARSA8J/VeilVPN/releases/latest/download/Mehrveil-1.6.2.apk) | Android 5.0+ (all CPUs) |
| 🖥️ Windows | [Mehrveil-1.6.2.exe](https://github.com/PARSA8J/VeilVPN/releases/latest/download/Mehrveil-1.6.2.exe) | Windows 10/11 64-bit |

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
3. Open Mehrveil → tap the big glass button. Done.

<details>
<summary><b>❓ "App not installed" — read this</b></summary>

**v1.6.2 ships under a brand-new app identity (`com.mehrveil.app`)** — it installs as a completely fresh app on every device, so **any previously installed VeilVPN (any signature) can no longer block it**. No uninstall needed.

What was fixed, version by version:
- **v1.6.2** → new package name + supports **Android 5.0+** (was 7.0+) + v1/v2/v3 signatures all embedded.
- **v1.6.1** → fixed the mis-packaged `resources.arsc` (was compressed; Android 11+ rejects that).
- Still failing? The downloaded file must be ~136 KB and named `Mehrveil-1.6.2.apk` — a truncated download produces the exact same error. If Play Protect asks, tap **Install anyway**.
- Verify the signature: certificate SHA-256
  `8c1859dbf7a7f95bfe18ba228182ec73dee9ebb30a603c76987923d83ef7812d`.
</details>

### Windows
1. Download `Mehrveil-1.6.2.exe` and run it — no installer, no browser, no dependencies.
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
bash scripts/build_apk.sh     # -> Mehrveil-1.6.2.apk
```

### Windows (native Rust)
```bash
rustup target add x86_64-pc-windows-gnu
cd windows-rs && cargo test   # end-to-end protocol tests against simulated servers
cd .. && bash scripts/build_exe.sh   # -> Mehrveil-1.6.2.exe
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

## 🛡️ فارسی — Mehrveil (قبلاً VeilVPN)

**Mehrveil** یک کلاینت VPN متن‌باز و رایگان برای **اندروید و ویندوز** با تم حرفه‌ای **Liquid Glass** است — هستهٔ VPN واقعی داخل خود برنامه، بدون نیاز به هیچ اپ دیگری.

- 📱 **اندروید** (`Mehrveil-1.6.2.apk`) — VpnService بومی + استک TCP/IP یوزر-اسپیس + UI شیشه‌ای؛ بدون v2rayNG و برنامهٔ جانبی.
- 🖥️ **ویندوز** (`Mehrveil-1.6.2.exe`) — اپ کاملاً **نیتیو Rust** (بدون مرورگر!) با بلور واقعی Acrylic + هستهٔ پروکسی داخل خود exe + ست‌شدن خودکار پروکسی سیستم.
- 📡 **سرورها با پرچم، اسم کشور و پینگ واقعی** — لیست از قبل داخل برنامه است + به‌روزرسانی زنده؛ پینگ هر سرور واقعاً اندازه‌گیری می‌شود و به‌صورت پیش‌فرض **۲۴ سرور سریع‌تر** را می‌بینید.
- 📊 **آمار واقعی** — سرعت و پینگ از شمارنده‌های واقعی تونل، نه عدد تقلبی.
- 🔁 Failover خودکار · 🔒 ضد DNS leak · 🌍 ۱۶ زبان (پیش‌فرض انگلیسی) · ⚙️ سابسکریپشن دلخواه
- 🔓 **۳ روز آزمایش رایگان خودکار**؛ کلید پریمیوم از ربات [`@veilvpnverifybot`](https://t.me/veilvpnverifybot)

**پروتکل‌ها:** VLESS (+WS/TLS)، Trojan، Shadowsocks (AES-GCM)

### 📥 دانلود

| پلتفرم | فایل | نیاز |
|---|---|---|
| 📱 اندروید | [Mehrveil-1.6.2.apk](https://github.com/PARSA8J/VeilVPN/releases/latest/download/Mehrveil-1.6.2.apk) | اندروید ۵+ |
| 🖥️ ویندوز | [Mehrveil-1.6.2.exe](https://github.com/PARSA8J/VeilVPN/releases/latest/download/Mehrveil-1.6.2.exe) | ویندوز ۱۰/۱۱ ۶۴ بیتی |

### ❓ «App not installed» — عیب‌یابی

**نسخهٔ ۱.۶.۲ با هویت کاملاً جدید (`com.mehrveil.app`) منتشر شده** — روی هر دستگاهی به‌عنوان یک اپ تازه نصب می‌شود و **نسخه‌های قبلی VeilVPN (با هر امضایی) دیگر مانع نصب نیستند**. دیگر نیازی به پاک‌کردن نسخهٔ قبلی نیست.

- **۱.۶.۲** → اسم و پکیج جدید + پشتیبانی از **اندروید ۵ به بالا** (قبلاً ۷ بود) + امضای v1/v2/v3
- **۱.۶.۱** → فیکس بسته‌بندی `resources.arsc` که اندروید ۱۱+ ردش می‌کرد
- اگر باز نشد: فایل باید حدود ۱۳۶ کیلوبایت و با نام `Mehrveil-1.6.2.apk` باشد — دانلود ناقص همین ارور را می‌دهد. اگر Play Protect هشدار داد → «نصب همان‌جا / Install anyway».
### 📄 مجوز

MIT — کامل آزاد.

</div>
