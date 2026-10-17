# VeilVPN 1.6.0

<p align="center"><b>See nothing. Be invisible.</b> — کلاینت VPN با رابط شیشه‌ای مایع (Liquid Glass)، هستهٔ واقعی داخل خود برنامه، بدون نیاز به هیچ اپ دیگری.</p>

<div dir="rtl">

## این چیه؟

VeilVPN یک کلاینت VPN متن‌باز برای **اندروید و ویندوز** است:

- 📱 **اندروید** (`VeilVPN-1.6.0.apk`) — WebView با تم Liquid Glass + هستهٔ VPN واقعی بومی (VpnService + استک TCP/IP یوزر-اسپیس). بدون v2rayNG و هیچ برنامهٔ جانبی.
- 🖥️ **ویندوز** (`VeilVPN-1.6.0.exe`) — اپ **نیتیو Rust** با رابط گرافیکی Liquid Glass (بدون مرورگر!) + هستهٔ پروکسی واقعی داخل خود exe؛ با دکمهٔ اتصال، **پروکسی سیستم** خودکار ست می‌شود.

## امکانات

- 🔓 **بدون نیاز به کلید برای شروع** — خودش ۳ روز آزمایش رایگان روی دستگاه می‌سازد (کلید پریمیوم از ربات تلگرام: `t.me/veilvpnverifybot`)
- 📡 **لیست سرورها هیچ‌وقت خالی نمی‌ماند** — صدها سرور سالم (۳۰+ کشور) از قبل داخل برنامه است + به‌روزرسانی زنده از چند منبع عمومی با آینهٔ CDN
- 🔁 **Failover خودکار** — اگر سروری از کار افتاد، تا ۵ سرور جایگزین امتحان می‌شود
- ⚙️ سابسکریپشن دلخواه (لینک sub خودت را در تنظیمات بده)
- 🌍 ۱۶ زبان — پیش‌فرض انگلیسی، فارسی راست‌چین کامل
- 🪟 تم **Liquid Glass** حرفه‌ای (روی ویندوز: بلور واقعی Acrylic پشت پنجره)
- 🔒 DNS از داخل تونل (ضد DNS leak)
- پروتکل‌ها: **VLESS** (+WebSocket/TLS)، **Trojan**، **Shadowsocks** (AES-GCM)

## 📥 دانلود مستقیم (نسخهٔ نهایی)

| پلتفرم | لینک مستقیم |
|---|---|
| 📱 اندروید ۷+ | [VeilVPN-1.6.0.apk](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.0.apk) |
| 🖥️ ویندوز ۱۰/۱۱ | [VeilVPN-1.6.0.exe](https://github.com/PARSA8J/VeilVPN/releases/latest/download/VeilVPN-1.6.0.exe) |

> اگر دانلود از GitHub برایت باز نشد، یک VPN/پروکسی موقت روشن کن یا از صفحهٔ [Releases](../../releases) فایل‌ها را بگیر. آخرین نسخه همیشه روی `releases/latest` است.

| پلتفرم | نیاز | نصب |
|---|---|---|
| اندروید | Android 7+ (همه‌ی CPUها) | نصب مستقیم APK، اجازهٔ «منابع ناشناس». اگر Play Protect هشدار داد → «نصب همان‌جا» |
| ویندوز | Windows 10/11 64-bit | فقط اجرا کنید — UI نیتیو باز می‌شود؛ نیازی به مرورگر نیست |

## ساخت از سورس

### اندروید
پیش‌نیاز فقط جاوا/پایتون است؛ بقیهٔ ابزارها را `setup_tools.sh` خودش دانلود می‌کند.

```bash
bash scripts/setup_tools.sh
bash scripts/build_apk.sh          # -> VeilVPN-1.6.0.apk
```

### ویندوز (Rust نیتیو)
پیش‌نیاز: Rust + هدف windows-gnu (و روی لینوکس: llvm-mingw).

```bash
rustup target add x86_64-pc-windows-gnu
# تست‌ها (شامل تست end-to-end پروتکل‌ها روی سرورهای شبیه‌سازی‌شده):
cd windows-rs && cargo test
# بیلد:
bash scripts/build_exe.sh          # -> VeilVPN-1.6.0.exe
```

### سرورهای seed
بازسازی لیست داخلی سرورها:
```bash
python3 scripts/fetch_seeds.py && python3 scripts/select_seeds.py
```

## ساختار مخزن

```
shared/       رابط وب شیشه‌ای (پایهٔ UI اندروید) + ۱۶ زبان
android/      اپ اندروید: VpnService + استک TCP/IP + کلاینت‌های VLESS/Trojan/SS
windows-rs/   اپ ویندوز نیتیو (Rust + egui، تم Liquid Glass) + تست‌های e2e
scripts/      اسکریپت‌های بیلد (APK / EXE / seed / آیکون)
tests/        وب‌استاب برای تست UI
```

CI روی هر push خودش APK و EXE می‌سازد (تب Actions)؛ هر تگ `v*` خودکار Release می‌شود.

## مجوز

MIT — کامل آزاد.
</div>

---

**English:** VeilVPN is an open-source VPN client for **Android & Windows** with a professional **Liquid Glass** UI. Android ships a native VpnService core (user-space TCP/IP stack) behind a WebView UI; Windows is a **fully native Rust app** (egui, acrylic blur-behind, single ~10MB exe) with a real in-app proxy core (VLESS/WS/TLS, Trojan, Shadowsocks-AEAD), automatic system-proxy switching, auto 3-day trial license, baked-in server list + live updates, failover, and 16 languages (English default). Protocols: VLESS (+WS/TLS), Trojan, Shadowsocks (AES-GCM). Build from source with `scripts/build_apk.sh` / `scripts/build_exe.sh`. MIT licensed.
