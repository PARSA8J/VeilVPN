# VeilVPN 1.5.1

<p align="center"><b>See nothing. Be invisible.</b> — کلاینت VPN با رابط شیشه‌ای، هستهٔ واقعی داخل خود برنامه، بدون نیاز به هیچ اپ دیگری.</p>

<div dir="rtl">

## این چیه؟

VeilVPN یک کلاینت VPN متن‌باز برای **اندروید، ویندوز و وب** است:

- 📱 **اندروید** (`VeilVPN-1.5.1.apk`) — WebView همان رابط وب محبوب + هستهٔ VPN واقعی بومی (VpnService + استک TCP/IP یوزر-اسپیس). بدون v2rayNG و هیچ برنامهٔ جانبی.
- 🖥️ **ویندوز** (`VeilVPN-1.5.1.exe`) — یک فایل exe بدون نصب؛ UI در مرورگر باز می‌شود، هستهٔ پروکسی واقعی داخل خود exe است و با دکمهٔ اتصال، **پروکسی سیستم** خودکار ست می‌شود.
- 🌐 **وب** — همان رابط به‌صورت استاتیک (`shared/`) روی هر هاستی قابل اجراست.

## امکانات

- 🔓 **بدون نیاز به کلید برای شروع** — خودش ۳ روز آزمایش رایگان روی دستگاه می‌سازد (کلید پریمیوم از ربات تلگرام: `t.me/veilvpnverifybot`)
- 📡 **لیست سرورها هیچ‌وقت خالی نمی‌ماند** — صدها سرور سالم (۳۰+ کشور) از قبل داخل برنامه است + به‌روزرسانی زنده از چند منبع عمومی با آینهٔ CDN
- 🔁 **Failover خودکار** — اگر سروری از کار افتاد، تا ۵ سرور جایگزین امتحان می‌شود + watchdog سلامت اتصال
- ⚙️ سابسکریپشن دلخواه (لینک sub خودت را در تنظیمات بده)
- 🌍 ۱۶ زبان + فارسی راست‌چین کامل
- 🔒 DNS از داخل تونل (ضد DNS leak) + فیلتر AAAA
- پروتکل‌ها: **VLESS** (+WebSocket/TLS)، **Trojan**، **Shadowsocks** (AES-GCM)

## دانلود

از صفحهٔ [Releases](../../releases) آخرین `VeilVPN-1.5.1.apk` و `VeilVPN-1.5.1.exe` را بگیرید
(یا از Actions هر build فایل artifact دارد).

| پلتفرم | نیاز | نصب |
|---|---|---|
| اندروید | Android 7+ (همه‌ی CPUها) | نصب مستقیم APK، اجازهٔ «منابع ناشناس» |
| ویندوز | Windows 10/11 64-bit | فقط اجرا کنید؛ UI در مرورگر باز می‌شود |
| وب | هر مرورگر | `shared/index.html` را host کنید (حالت دمو) |

## ساخت از سورس

پیش‌نیاز فقط جاوا/پایتون/گیت است؛ بقیهٔ ابزارها را `setup_tools.sh` خودش دانلود می‌کند.

```bash
bash scripts/setup_tools.sh     # دانلود build-tools + platform + ecj + go (~130MB، یک‌بار)
bash scripts/build_apk.sh       # خروجی: VeilVPN-1.5.1.apk
bash scripts/build_exe.sh       # خروجی: VeilVPN-1.5.1.exe (کراس‌کامپایل ویندوز)
```

- `shared/` — رابط وب مشترک هر ۳ پلتفرم (index.html / app.js / glass.css / i18n.js / seed.txt)
- `android/` — سورس جاوای اپ (WebView + پل `VeilNative` + هستهٔ VPN)
- `windows/` — سورس Go یک‌فایلیِ exe (UI سرور محلی + هستهٔ پروکسی + پروکسی سیستم)
- `scripts/` — اسکریپت‌های ساخت و انتخاب seed
- `tests/webstub/` — استاب پل برای تست UI بدون دستگاه

### معماری اندروید

```
┌─ WebView (رابط وب، بدون اینترنت برای رندر) ─┐
│  window.VeilNative.api(action, payload, cb) │
└──────────────┬──────────────────────────────┘
        پل JS ↔ جاوا (thread-safe، callback + push event)
┌──────────────┴──────────────────────────────┐
│  VpnService → TUN (MTU 8500, 0.0.0.0/0)     │
│  TunPump → TcpFlow (TCP یوزر-اسپیس)         │
│  DnsAgent (DNS داخل تونل، fake-IP)          │
│  VLESS / Trojan / SS-AEAD (جاوی خالص)       │
└─────────────────────────────────────────────┘
```

## نکات و محدودیت‌ها

- `vmess://` پشتیبانی نمی‌شود (در لیست فیلتر می‌شود)؛ Reality و xtls-vision هم فعلاً خیر
- ویندوز در حالت «پروکسی سیستم» کار می‌کند (مرورگرها و اکثر برنامه‌ها امن می‌شوند)؛ حالت TUN تمام-دستگاه در نقشهٔ راه است
- سرورها از لیست‌های عمومی رایگان می‌آیند و ممکن است هر روز تغییر کنند — سابسکریپشن شخصی خودتان را در تنظیمات بدهید
- آمار دانلود/آپلود نمایشی است؛ پینگ سرورها واقعی است

## لایسنس

MIT — آزاد برای هر استفاده. فقط محصول را با نام خودتان نفروشید 😉

</div>

---

<div dir="ltr" align="center">

**English summary** — VeilVPN is an open-source VPN client (Android · Windows · Web) with a glassmorphism UI and a built-in real core: Android = WebView UI + VpnService with a user-space TCP/IP stack (VLESS/Trojan/SS-AEAD in pure Java, DNS through tunnel, auto failover); Windows = single portable EXE that serves the same UI locally and toggles the system proxy through a real VLESS/Trojan/SS core. No v2rayNG or external apps needed. Auto-provisioned 3-day trial, baked-in seed servers, CDN-mirrored live lists, 16 languages with full RTL Persian. Build: `bash scripts/setup_tools.sh && bash scripts/build_apk.sh && bash scripts/build_exe.sh`. License: MIT.

</div>
