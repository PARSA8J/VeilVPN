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
| 📱 Android | [Mehrveil-1.6.10.apk](https://parsa8j.github.io/VeilVPN/Mehrveil-1.6.10.apk) — **direct link** · [GitHub release](https://github.com/PARSA8J/VeilVPN/releases/download/v1.6.10/Mehrveil-1.6.10.apk) | Android 5.0+ (all CPUs) |
| 🖥️ Windows | [Mehrveil-1.6.10.exe](https://parsa8j.github.io/VeilVPN/Mehrveil-1.6.10.exe) — **direct link** · [GitHub release](https://github.com/PARSA8J/VeilVPN/releases/download/v1.6.10/Mehrveil-1.6.10.exe) | Windows 10/11 64-bit |

> ⚠️ **Old releases have been removed** — only **v1.6.10** is distributed. Download **only** files named `Mehrveil-1.6.10.apk` / `Mehrveil-1.6.10.exe`.
> ✅ File integrity — the correct APK is exactly **119,890 bytes (~117 KB)** · SHA-256 `71f4f2f57fab9326ca3196853c8a7be5190402fed0ecea85e55e16ef6c198fc8`. A different size = corrupted download → re-download from the direct link.
> 🪟 Windows EXE integrity — **1,666,048 bytes (~1.6 MB)** · SHA-256 `5d9edde2a7e17f4e78d8d77d1d7afdc2b22c03bcd6191ddcdccb4149dfccb3b8`.
> 🧹 **Running an old file?** If the app window shows a version older than **v1.6.10** in its footer (e.g. `v1.5.4 - MIT`), you launched an old downloaded installer — delete every old Mehrveil/VeilVPN file from your Downloads folder and download v1.6.10 again.

> 💡 If GitHub is blocked on your network, turn on any temporary proxy, or grab the files from the [Releases page](../../releases). The latest version always lives at `releases/latest`.

## ✨ Features

- **🪟 Liquid Glass UI** — a real glassmorphism design language: layered translucent panels, specular highlights, soft shadows. Windows renders it with the built-in **WebView2 (Chromium) engine** — DirectComposition, no GPU hacks — while the VPN core stays fully **native Rust** in the same EXE.
- **📡 Always-ready servers** — a curated list of live public servers (30+ countries) is baked into the app, plus live refresh from multiple public aggregator sources with CDN mirrors. The app **measures real ping to every server** (TCP handshake time) and shows the **country flag + country name + latency** for each one. By default you see only the **24 fastest** — tap *Show all* for the complete list.
- **📊 Real stats** — live download/upload speed from actual tunnel byte counters and true round-trip ping to your connected server. Nothing simulated.
- **🔁 Automatic failover** — if a server dies mid-session, up to 5 candidates are retried without you noticing.
- **🌍 16 languages** — English by default; switch anytime in Settings (full RTL for Persian, Arabic, Hebrew, Urdu).
- **🔐 Anti DNS-leak** — all DNS queries are resolved inside the encrypted tunnel; IPv6 is handled safely so nothing leaks outside.
- **⚙️ Custom subscription** — paste your own subscription link (V2Ray/Clash style) in Settings to use your private servers.
- **🔓 100% free, forever** — no registration, no keys, no trials, no limits. MIT-licensed open source.

**Protocols:** VLESS (+ WebSocket / TLS), Trojan, Shadowsocks (AEAD ciphers)

## 📲 Install

### Android
1. Download the APK and open it (allow *"Install unknown apps"* for your browser).
2. If Play Protect shows a warning → **Install anyway** (the APK is signed with the project's public key; verify below).
3. Open Mehrveil → tap the big glass button. Done.

<details>
<summary><b>❓ "App not installed" — read this</b></summary>

**v1.6.2+ ships under a brand-new app identity (`com.mehrveil.app`)** — it installs as a completely fresh app on every device, so **any previously installed VeilVPN (any signature) can no longer block it**. No uninstall needed.

What was fixed, version by version:
- **v1.6.3** → fresh build + new version code (clears any stuck install session on the device).
- **v1.6.2** → new package name + supports **Android 5.0+** (was 7.0+) + v1/v2/v3 signatures all embedded.
- **v1.6.1** → fixed the mis-packaged `resources.arsc` (was compressed; Android 11+ rejects that).

If the installer shows the app name + icon but still says **«App not installed»**, your PHONE is blocking the install — the file itself is fine (a broken file can't show the app name/icon). Fix it on the phone:
1. **Samsung — Auto Blocker** (top cause on One UI): Settings → **Security and privacy → Auto Blocker** → turn it **off** (or disable *Block app installs*), then install again.
2. **Play Protect**: Play Store → profile icon → **Play Protect** → gear icon → turn **off** *Scan apps with Play Protect* → install → turn it back on after.
3. **Airplane-mode trick**: turn on Airplane mode, install the APK (Play Protect can't fetch a remote verdict), turn it off after.
4. **Reboot** the phone, make sure ≥200 MB free storage, then install.
- Verify the signature: certificate SHA-256
  `8c1859dbf7a7f95bfe18ba228182ec73dee9ebb30a603c76987923d83ef7812d`.
</details>

### Windows
1. Download `Mehrveil-1.6.10.exe` and run it — no installer. The UI uses Windows' built-in WebView2 engine (present on Windows 11 and nearly all Windows 10 systems; if missing, the app shows a direct download link).
2. Tap connect; the **system proxy** is configured automatically, and removed cleanly on disconnect.

## 🆕 What's new in v1.6.10

- **The dead UI is fixed.** On Windows the local UI server required the security token on *every* request, but the browser fetches the page's own scripts without one — so `app.js`/`i18n.js` were rejected and **no interface code ran at all**: buttons did nothing and the server list stayed empty. The token is now injected into the scripts at serve time; API calls stay token-protected.
- **Servers appear instantly, always** — a synchronous snapshot (status + nodes + real pings) is served on every launch and reload. Subscription refreshes run off the main engine loop, so Connect/Disconnect stay responsive even when subscription sources are blocked or slow (8 s cap per source).
- **Deeper Liquid Glass + fully responsive** — the new specular recipe (blurred inner ring, 45° gloss, contrast layer on the connect orb) across all panels and buttons; layout adapts to short windows, landscape and narrow phones. Android gets the same shared UI upgrade.
- **Self-healing + diagnosable:** the UI falls back to the page's own token if WebView2 script injection ever fails, retries its first fetch, and every JavaScript error lands in `%APPDATA%\Mehrveil\log.txt`.

## 🆕 What's new in v1.6.9

- **Fix for the "can't reach this page" startup error.** On many machines v1.6.8 opened with a browser-style error page (`http://127.0.0.1:0` — ERR_UNSAFE_PORT) instead of the app: the window was pointed at the internal UI server before that server had picked its port. v1.6.10 binds the port **before** the window is created and can never navigate to an invalid address — plus a safe native error dialog instead of any browser error page, ever.
- **Everything else is exactly v1.6.8** — same WebView2 Liquid Glass UI, same Rust core, same features. If v1.6.8 showed you an error page instead of the app, install this one.

## 🆕 What's new in v1.6.8

- **Windows white screen: killed for good — the UI engine is replaced.** Three fix rounds on the old GPU-rendered interface (transparent window removal, DirectX 12/Vulkan + OpenGL double renderer, paint-before-show boot) proved that the *whole* rendering stack was the problem on affected machines. So v1.6.8 drops it entirely: the desktop app now renders its interface with **WebView2 — the same Chromium engine that already works in your Edge browser and in the Android app**. It draws through DirectComposition, not through the OpenGL/Vulkan paths that white-screened. If your PC can show a web page, it can show Mehrveil.
- **Same Liquid Glass design, same 16 languages.** The interface is the exact same glass UI as before — flags, real pings, timer, live speeds — now drawn by Chromium instead of egui.
- **Still a real Rust core.** The proxy engine, VLESS/Trojan/SS protocol clients, subscription fetching, failover, and the system-proxy switch are the same native Rust code — no third-party VPN app, no scripts.
- **New real stats + verified exit IP.** While connected, the app measures **true up/down byte counters** from inside the tunnel and performs a **real IP check through the tunnel** (Cloudflare trace over TLS) — the connected node gets an honest *verified* mark.
- **Fail-fast diagnostics stay:** `%APPDATA%\Mehrveil\log.txt` startup log, `Mehrveil.exe --diag`, native error dialogs, single-instance guard.
- **One honest requirement:** Windows 10/11 with the WebView2 Runtime (pre-installed on Windows 11 and on virtually all updated Windows 10 systems). If it is missing, the app tells you exactly what to install instead of showing a broken window. The EXE is now also ~4× smaller because the UI engine ships with Windows.


### 🆕 v1.6.10 چه چیزی عوض شد؟

- **رفع خرابی رابط کاربری.** سرور داخلی روی همهٔ درخواست‌ها توکن می‌خواست ولی مرورگر اسکریپت‌های خود صفحه را بدون توکن می‌خواند — پس `app.js` و `i18n.js` رد می‌شدند و **هیچ کدی از رابط اجرا نمی‌شد**: دکمه‌ها بی‌اثر و لیست سرور خالی بود. حالا توکن هنگام سرو شدن داخل اسکریپت‌ها تزریق می‌شود و API همچنان محافظت‌شده است.
- **نمایش فوری و همیشگی سرورها** — اسنپ‌شات همگام (وضعیت + نودها + پینگ واقعی) در هر اجرا و رفرش سرو می‌شود؛ رفرش سابسکریپشن از حلقهٔ اصلی موتور خارج شده تا Connect/Disconnect همیشه پاسخ‌گو بمانند (سقف ۸ ثانیه برای هر منبع).
- **شیشه‌ای‌تر + کاملاً واکنش‌گرا** — دستور جدید شیشهٔ مایع روی همهٔ پنل‌ها و دکمه‌ها؛ چیدمان با پنجرهٔ کوتاه، حالت افقی و گوشی باریک هم سازگار است. اندروید هم همین ارتقا را می‌گیرد.
- **خودترمیم + قابل عیب‌یابی:** اگر تزریق اسکریپت WebView2 شکست بخورد رابط از توکن خود صفحه استفاده می‌کند، دریافت اولیه تا ۱۰ بار تلاش می‌شود و هر خطای JS در `%APPDATA%\Mehrveil\log.txt` ثبت می‌شود.

### 🆕 v1.6.9 چه چیزی عوض شد؟

- **رفع ارور «can't reach this page» هنگام باز شدن برنامه.** در v1.6.8 روی خیلی از سیستم‌ها به‌جای برنامه یک صفحهٔ خطای مرورگر باز می‌شد (`http://127.0.0.1:0` — ERR_UNSAFE_PORT): پنجره قبل از آماده‌شدن پورتِ سرور داخلی به آن آدرس می‌رفت. در v1.6.10 پورت قبل از ساخته‌شدن پنجره قطعی می‌شود و برنامه هیچ‌وقت به آدرس نامعتبر نمی‌رود — و در بدترین حالت هم به‌جای صفحهٔ خطای مرورگر، یک پیام بومی واضح نشان داده می‌شود.
- **بقیهٔ چیزها دقیقاً مثل v1.6.8 است** — همان رابط Liquid Glass با WebView2، همان هستهٔ Rust. اگر v1.6.8 برای شما صفحهٔ خطا نشان می‌داد، همین نسخه را نصب کنید.

### 🆕 v1.6.8 چه چیزی عوض شد؟

- **صفحهٔ سفید ویندوز برای همیشه مرده — موتور رابط کاربری عوض شد.** سه راند فیکس روی رندر قبلی (حذف پنجرهٔ شفاف، رندر دوگانهٔ DirectX 12/Vulkan + OpenGL، بوت paint-before-show) ثابت کرد که *کل* استک رندر مقصر است. پس v1.6.8 کلاً کنارش گذاشت: رابط دسکتاپ حالا با **WebView2 رندر می‌شود — همان موتور Chromium که در مرورگر Edge و اپ اندروید از قبل درست کار می‌کند**. اگر PC شما می‌تواند یک صفحهٔ وب را نشان دهد، می‌تواند Mehrveil را هم نشان دهد.
- **همان طراحی Liquid Glass، همان ۱۶ زبان** — پرچم‌ها، پینگ واقعی، تایمر و سرعت زنده، این بار با Chromium به‌جای egui.
- **هستهٔ واقعی Rust سر جایش است** — موتور پروکسی، کلاینت‌های VLESS/Trojan/SS، دریافت سابسکریپشن، فیلور و سوییچ پروکسی سیستم، همان کد نیتیو قبلی است.
- **آمار واقعی جدید + تأیید IP خروجی.** هنگام اتصال، شمارندهٔ واقعی بایت‌های بالا/پایین داخل تونل اندازه‌گیری می‌شود و IP خروجی واقعی از داخل تونل چک می‌شود (Cloudflare trace روی TLS) — سرور متصل نشان «تأیید شده» می‌گیرد.
- **عیب‌یابی سریع سر جایش است:** لاگ `%APPDATA%\Mehrveil\log.txt`، دستور `Mehrveil.exe --diag`، پیام خطای بومی و جلوگیری از اجرای دوباره.
- **تنها پیش‌نیاز:** ویندوز ۱۰/۱۱ با WebView2 Runtime (روی ویندوز ۱۱ و اکثر ویندوزهای ۱۰ به‌روزشده از قبل هست). اگر نبود، برنامه دقیقاً می‌گوید چه چیزی را نصب کنید — نه اینکه پنجرهٔ خراب نشان دهد. حجم EXE هم ~۴ برابر کوچک‌تر شد.

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
bash scripts/build_apk.sh     # -> Mehrveil-1.6.10.apk
```

### Windows (Rust core + WebView2 shell — MSVC)
```bash
cd windows-rs && cargo test        # e2e protocol tests + UI-bridge contract test
cd .. && bash scripts/build_exe.sh # -> Mehrveil-1.6.10.exe  (needs the MSVC toolchain — as on windows-latest CI)
```

### Server seed list
```bash
python3 scripts/build_verified_seed.py   # fetch -> parse -> TCP test -> full protocol verify -> seed + sub.txt
```

## 📁 Repository layout

```
shared/       Liquid Glass web UI (Android) + i18n for 16 languages
android/      Android app: VpnService + user-space TCP/IP stack + VLESS/Trojan/SS clients
windows-rs/   Windows app: Rust core (engine/protocols/proxy) + WebView2 UI shell + tests
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

- 📱 **اندروید** (`Mehrveil-1.6.10.apk`) — VpnService بومی + استک TCP/IP یوزر-اسپیس + UI شیشه‌ای؛ بدون v2rayNG و برنامهٔ جانبی.
- 🖥️ **ویندوز** (`Mehrveil-1.6.10.exe`) — هستهٔ VPN کاملاً **نیتیو Rust** داخل خود exe (بدون هیچ اپ جانبی) + رابط Liquid Glass با موتور **WebView2 ویندوز** (رندر DirectComposition — روی هر GPU/درایوری درست نمایش داده می‌شود) + ست‌شدن خودکار پروکسی سیستم.
- 📡 **سرورها با پرچم، اسم کشور و پینگ واقعی** — لیست از قبل داخل برنامه است + به‌روزرسانی زنده؛ پینگ هر سرور واقعاً اندازه‌گیری می‌شود و به‌صورت پیش‌فرض **۲۴ سرور سریع‌تر** را می‌بینید.
- 📊 **آمار واقعی** — سرعت و پینگ از شمارنده‌های واقعی تونل، نه عدد تقلبی.
- 🔁 Failover خودکار · 🔒 ضد DNS leak · 🌍 ۱۶ زبان (پیش‌فرض انگلیسی) · ⚙️ سابسکریپشن دلخواه
- 🔓 **صددرصد رایگان، برای همیشه** — بدون ثبت‌نام، بدون کلید، بدون آزمایش و محدودیت. مجوز MIT متن‌باز.

**پروتکل‌ها:** VLESS (+WS/TLS)، Trojan، Shadowsocks (AES-GCM)

### 📥 دانلود

| پلتفرم | فایل | نیاز |
|---|---|---|
| 📱 اندروید | [Mehrveil-1.6.10.apk](https://parsa8j.github.io/VeilVPN/Mehrveil-1.6.10.apk) — **لینک مستقیم** · [ریلیز گیت‌هاب](https://github.com/PARSA8J/VeilVPN/releases/download/v1.6.10/Mehrveil-1.6.10.apk) | اندروید ۵+ |
| 🖥️ ویندوز | [Mehrveil-1.6.10.exe](https://parsa8j.github.io/VeilVPN/Mehrveil-1.6.10.exe) — **لینک مستقیم** · [ریلیز گیت‌هاب](https://github.com/PARSA8J/VeilVPN/releases/download/v1.6.10/Mehrveil-1.6.10.exe) | ویندوز ۱۰/۱۱ ۶۴ بیتی |

> ⚠️ **ریلیزهای قدیمی حذف شدند** — فقط **v1.6.10** توزیع می‌شود. فقط فایل‌های `Mehrveil-1.6.10.apk` / `Mehrveil-1.6.10.exe` را دانلود کن.
> ✅ کنترل سلامت فایل — فایل درست دقیقاً **۱۱۹٬۸۹۰ بایت (~۱۱۷ کیلوبایت)** است · اگر حجم فایل دانلودشده فرق داشت، دانلود ناقص بوده → از لینک مستقیم دوباره بگیر.
> 🧹 **فایل قدیمی اجرا کرده‌ای؟** اگر پایین پنجرهٔ برنامه نسخه‌ای قدیمی‌تر از **v1.6.10** نوشته (مثلاً `v1.5.4 - MIT`)، یعنی فایل نصبی قدیمی را باز کرده‌ای — همهٔ فایل‌های قدیمی Mehrveil/VeilVPN را از پوشهٔ Downloads پاک کن و v1.6.10 را دوباره دانلود کن.

### ❓ «App not installed» — عیب‌یابی

**اگر نصب‌کننده اسم و آیکون اپ را نشان می‌دهد ولی باز «App not installed» می‌دهد، فایل سالم است و گوشی‌ات جلوی نصب را می‌گیرد.** به‌ترتیب این‌ها را امتحان کن:

1. **سامسونگ — Auto Blocker** (مهم‌ترین دلیل در One UI): Settings → **Security and privacy → Auto Blocker** → خاموشش کن (یا فقط گزینهٔ *Block app installs* را بردار) → دوباره نصب کن.
2. **Play Protect**: اپ Google Play → عکس پروفایل → **Play Protect** → آیکون چرخ‌دنده → *Scan apps with Play Protect* را **خاموش** کن → نصب کن → بعد دوباره روشنش کن.
3. **ترفند حالت هواپیما**: Airplane mode را روشن کن، فایل را نصب کن (Play Protect نمی‌تواند آنلاین چک کند)، بعد هواپیما را خاموش کن.
4. گوشی را **ری‌استارت** کن و حداقل ۲۰۰ مگابایت فضای خالی داشته باش، بعد نصب کن.

نکات فنی:

- **۱.۶.۳** → بیلد تازه + versionCode جدید (هر نصب نیمه‌کارهٔ گیرکرده را هم پاک می‌کند)
- **۱.۶.۲** → هویت و پکیج جدید (`com.mehrveil.app`، بدون تداخل با هیچ نسخهٔ قبلی) + **اندروید ۵ به بالا** + امضای v1/v2/v3
- **۱.۶.۱** → فیکس بسته‌بندی `resources.arsc` که اندروید ۱۱+ ردش می‌کرد

### 📄 مجوز

MIT — کامل آزاد.

</div>
