# Aether Desktop 1.3.0 — Release notes

## What's new in 1.3.0

The app version stays **1.3.0**. This release brings three features over from Aether for Android **1.5.0** — **Custom configs**, **Zero Trust sign-in** and the **redesigned LAN share** — with the same behaviour and wording as on the phone, and bundles the **unmodified Aether Core 2.3.0**. Android's *phone hotspot* share mode is intentionally **not** part of the Windows edition. Saved profiles and the chosen backend are kept.

### Custom configs (new, off by default)

Ported from Aether for Android **1.5.0**. Settings → **Custom configs**. While the switch is off (the default) nothing changes: the six existing modes, profiles and the engine behave exactly as before.

- **What you can import.** Share links `vless://`, `vmess://`, `trojan://`, `ss://`, `socks://` / `socks5://`, `wireguard://` / `wg://`; a full **Xray JSON** config (any outbound Xray supports, e.g. Hysteria2, XHTTP, Reality); Base64 or plain **subscription** text; a **subscription URL** (`https://` only, at most 5 redirects, 2 MB, 20 s; when connected it is fetched through the tunnel); and **NapsternetV / Npv Tunnel `.npvt` files**.
- **How you import.** Paste into the box and press **Import**, pick a file, or drag a file onto the drop zone. **There is no QR scanning on Windows** (no camera path in the desktop app); paste the link instead.
- **`.npvt` locks are respected.** The file is decoded byte-for-byte like Android. A config locked to another device, an expired one, or one marked *mobile only* is rejected with a clear reason; a password-protected file asks for its password. A locked config's raw link is never shown, copied or edited.
- **Saved list.** Select, rename, edit (unlocked only), copy link, delete, and a separate **Delete all configs**. The list is sealed with Windows DPAPI (`custom_configs.bin` in the data folder); the web UI never sees the raw outbound.
- **15 chain modes.** The config can run alone, in front of, or behind the existing engines:

| Mode | Order (first hop → exit) |
|---|---|
| Config only | Config |
| Config → Aether / Aether → Config | xray first / engine first |
| Config → Psiphon / Psiphon → Config | |
| Config → Tor / Tor → Config | |
| Config → Aether → Psiphon / Config → Aether → Tor | config in front of a two-hop chain |
| Config → Tor → Psiphon / Config → Tor → Aether | |
| Aether → Psiphon → Config / Aether → Tor → Config | config as the exit of a two-hop chain |
| Tor → Psiphon → Config / Tor → Aether → Config | |

- **Connection card.** The protocol tile shows the real chain, e.g. `VLESS → Tor` or `Tor → Psiphon → Trojan`, and is shortened to fit (transport names such as MASQUE, WireGuard, Gool, MIM).
- **Xray core v25.12.8**, bundled as `engine\xray\xray.exe` (x64 and x86 zips and binaries pinned by SHA-256 in `scripts/stage-xray.ps1`). Version 26 rejects `allowInsecure` and the mKCP `seed`, which many existing configs and older `.npvt` files still use, so the app stays on 25.12.8 like Android.
- **The `Config → Tor` fix.** Client-side sniffing is disabled in every generated config, and the hop that carries `tor.exe` splits Tor's ClientHello into small TLS records so that the *server's* sniffer cannot replace the relay address with Tor's fake SNI either (details below); unit tests lock this for all three positions.
- **Safety.** xray runs as a hidden child in the app's Job Object (killed with the app), listens on loopback only, gets a config file that is deleted when it stops, and is restarted at most 5 times. DNS stays inside the tunnel; when xray dials through the engine or Tor the server name is passed unresolved. The leak guard only opens the extra egress xray needs while a custom config runs. Split tunnelling stays off by default and the MTU default stays 1280.

### Custom configs: chain and ping fixes (1.3.0)

- **`Config → Psiphon` now goes through the config.** Psiphon was started without an upstream, so it dialled directly and the chain had "no tunnel underneath it". It now gets the config's local SOCKS port (`AETHER_UPSTREAM`); the log shows `psiphon dials out through the upstream proxy`.
- **Psiphon `WSAEACCES` (10013).** `Psiphon → Config` purges stale Aether firewall rules left by an earlier crash before it starts, and the egress filter lets the Psiphon/xray hop out. If Windows still answers *forbidden by its access permissions*, the app logs once which rule or profile (`BlockOutbound`) blocks it and what to do.
- **Correct pipeline label.** The log shows the real chain, e.g. `Pipeline: Config → Psiphon (custom config chain)`, instead of the base mode name.
- **Slow hops.** Xray timeouts for chains are handshake 30 s, idle 1800 s, uplink 10 s, downlink 30 s. When the config is the exit behind Tor/Psiphon, Mux (8 streams) is enabled; it is never used for XTLS Vision (`flow`), WireGuard or Hysteria.
- **Real ping.** Each saved config has a **Ping** button and the list has **Test all**. The test runs the config in a temporary loopback-only xray (password-protected SOCKS, up to 32 configs per run) and fetches `generate_204`. Results: a delay in ms, or `-1` with *No response*, *Server off*, *Filtered*, *Invalid config* or *No network*.
- **Diagnostics ping.** DoH / TLS checks always verify the certificate **and** the host name (no `danger_accept_invalid_hostnames`).
- **Privacy in logs.** Server addresses, host names, IPv6 and UUIDs from xray output are masked (`1.2.x.x`, `<host>`, `<uuid>`). Subscriptions are fetched through the tunnel when connected.
- **Clean paste.** Invisible characters (RLM/LRM, BOM, bidi marks, NBSP) that Telegram adds to copied links are removed on paste, import and subscription download.
- **Leftovers from 1.2.5.** `scripts/clean-legacy-1.2.5.py` removes files from 1.2.5 that no longer belong (old scripts, stale web bundles, files in `native/aether` not in `UPSTREAM-SHA256SUMS`); CI runs it before the core pristine check.

### Custom configs: chain, label and speed fixes (this build)

Measured against Aether for Android **1.5.0**, which uses the same method; healthy chains (`Config`, `Config → Aether`, `Aether → Config`, `Tor → Config`, `Config → Psiphon` and the regular six modes) are unchanged.

- **`Config → Tor`, `Config → Tor → Aether`, `Config → Tor → Psiphon` — root cause fixed.** `tor.exe` sends a made-up SNI (`www.<random>.com`) in every relay handshake. Most config servers (3x-ui / Marzban default: `sniffing` with `destOverride: tls`) read that name, replace the relay's IP with a domain that does not exist and drop the link — `unexpected eof while reading`, `TLS_ERROR`, Tor stuck at 10 % or never finishing. Android avoids it with a patched Arti that sends no SNI (`AETHER_TOR_LINK_SNI=off`). Windows now does the equivalent inside Xray: the hop that carries `tor.exe` (`tor-link`) splits Tor's ClientHello into 100–200 byte TLS records before it enters the config, so the server's sniffer cannot read a name and keeps the real relay address. Tested here with Tor 0.4.9 through the two attached `.npvt` sample configs: **Bootstrapped 100 % in 9–12 s** and `IsTor: true`, where the same configs stalled at 10 % before.
- **No bridge stall behind a config.** These chains logged *trying bridges* and waited the 420 s bridge window although bridges are never used when Tor already leaves through the config. They now use the normal window, like Android.
- **Faster Tor start.** A chained `tor.exe` no longer parses the GeoIP files (about 7 s on the reported PC before *Bootstrapped 0 %*); no country option is used in those torrc files.
- **`Psiphon → Config`.** The app probed Psiphon's SOCKS port with a connect/close every 200 ms, and Psiphon logged `socksPeekByte() failed: EOF` five times a second. The port is now latched once it is seen open. Psiphon's own direct-dial failures (`no route`, `i/o timeout`) come from the network, not the app.
- **Protocol names instead of "Config".** After import, the chain list, the Settings row and the connection card name the real protocol — `VLESS → Tor`, `Npvt → Aether`, `Tor → Aether(MASQUE) → Npvt` — in English in both languages. `.npvt` configs show **Npvt**, other links their protocol (VLESS, VMess, Trojan, SS, SOCKS, WireGuard, Hysteria).
- **Custom configs page redesigned like Android.** The native drop-down is replaced by a chain row that opens a sheet of rounded cards: the chain in large type, *Single · the config alone* / *Double chain* / *Triple chain* under it, the selected card with a blue frame and a check mark (keyboard: arrows, Enter, Esc). Saved configs are separate rounded cards; the selected one has a blue frame and a check.
- **Instant disconnect.** Removing the firewall rules and restoring the policies now run in parallel instead of one `netsh` call after another, also when stale rules are purged before a connection.
- **Less CPU, RAM and disk.** A line repeated in a row (e.g. a child process looping on one error) is written twice and then counted — `(previous line repeated N more times)` — and the log flusher writes each batch in one call.

### Security audit — Windows 1.3.0 compared with Android 1.5.0

Source-level review of this repository with the same ten areas and weights as the Android 1.5.0 audit (`docs/SECURITY_AUDIT_1.5.0.md`), plus the tests run for this build: 399 Rust unit tests on the Windows target (run under Wine), the frontend jsdom suites, `cargo check --target x86_64-pc-windows-gnu --all-targets`, and a real Tor bootstrap through the sample configs. It is not a penetration test; no leak test was run on Windows hardware.

| # | Area | Weight | Windows 1.3.0 | Android 1.5.0 | Note (Windows) |
|---|---|---:|---:|---:|---|
| 1 | Secrets & key management | 14 | 94 | 95 | AI key, Zero Trust sign-in, LAN-share password and saved configs sealed with DPAPI; nothing hardcoded. WARP identity files are plaintext. |
| 2 | Cryptography, TLS & MitM resistance | 13 | 95 | 96 | SPKI pins kept; diagnostics verify certificate **and** host name. A config's own `allowInsecure` is honoured, as on Android. |
| 3 | Data-leak risk (DNS, IPv6, WebRTC, bypass) | 17 | 89 | 90 | DNS inside the tunnel, WebRTC lock, IPv6 fail-closed, kill switch. The system proxy is not a universal route for arbitrary UDP apps. The new `tor-link` hop stays inside the config — no new egress. |
| 4 | LAN sharing surface | 10 | 87 | 86 | Zero-leak guard, optional RFC 1929 / Basic auth, no LAN/private targets, no bypass rules for guests. |
| 5 | Local storage at rest | 12 | 88 | 93 | `profile.json`, rotating log and WARP identity are plaintext; the Xray config file exists only while it runs. |
| 6 | Permissions & OS configuration | 7 | 92 | 97 | Requires administrator (firewall, WebRTC policy); rules are scoped and removed on disconnect, stale rules purged. |
| 7 | Logging & diagnostics | 7 | 96 | 97 | Public IPs, hosts and UUIDs masked; locked `.npvt` servers never logged; repeated lines collapsed. |
| 8 | Code quality & network configuration | 8 | 90 | 88 | Loopback-only listeners; capped restarts; Tor chains fixed at the root, guarded by unit tests. The chain's loopback SOCKS is unauthenticated (Android SB-5, still open on both). |
| 9 | On-device exposure (screen, clipboard) | 6 | 92 | 95 | A locked config's link cannot be copied or shown; an unlocked link copied by the user stays on the clipboard. |
| 10 | Supply chain & build integrity | 6 | 82 | 80 | Xray zip and binary pinned by SHA-256, core pristine check; installers not Authenticode-signed without a CI certificate. |
| | **Total** | **100** | **90.70 ≈ 91** | **91.96 ≈ 92** | |

The one point between the two comes from Windows-specific design (plaintext non-secret state, administrator rights, system proxy instead of a VpnService route), not from a known open vulnerability. No change in this build widens what leaves the machine: the Tor fix only changes how the ClientHello is cut into records inside the config's own tunnel.

### Updating a repository that is still on 1.2.5

Extract the ZIP, open the `Aether_Windows_V1.3.0` folder, select everything in it and copy it into the repository folder; when Explorer asks, choose **Replace the files in the destination**. Then **Commit** and **Push** in GitHub Desktop. Files of 1.2.5 that 1.3.0 no longer has (old scripts, old web bundles, `native/aether/quiche/fuzz` corpus folders, …) may stay in the repository: every CI job removes them first (`scripts/clean-legacy-1.2.5.py`), before any check or build. The five `quiche/fuzz/mayhem/*/testsuite` links of core 2.3.0 are left out of the ZIP on purpose: in 1.2.5 they are folders, and Explorer cannot put a file over a folder ("There is already a folder with the same name…"). They are fuzz-test links, unused by the build, and ignored by the core pristine check.

### Zero Trust: sign in before you connect

- **Sign in first.** Settings → Zero Trust → *Email code* → **Sign in** opens a dialog: the code is sent to the e-mail address, you type it (three tries), **Send a new code** becomes available after 30 seconds, and every problem (wrong team, wrong address, expired or wrong code, no network) is explained in plain words.
- **Sealed and visible.** The sign-in is sealed with Windows DPAPI, bound to the team **and** the e-mail address, and the panel shows whether it is still valid and until when. **Forget this sign-in** removes it. The token never reaches the web UI.
- **Code during connection.** If the engine asks for an e-mail code while connecting, a dialog opens over any page. **Not now** stops that connection attempt instead of leaving it waiting.
- **Membership.** An enrolled device shows its membership with **Remove this device’s membership** (with confirmation, only while disconnected); the next connection enrols again.
- **Service token and enrolment token.** *Service token* → **Test the service token** runs the same exchange the engine uses, without saving anything. A pasted *Access token* (enrolment token) is checked on the spot (empty, malformed, expired, or valid with time left).

### Share VPN on the local network

- **Redesigned Share page.** Live status (*off*, *waiting for a network*, *ready*, *port busy*), one **Share on this network** switch, a zero-leak badge, copyable SOCKS5 `10810` / HTTP `10811` / address rows, the computer's other addresses, **Copy all details**, setup steps and the check address.
- **Every real adapter.** The share listens on every Wi‑Fi and Ethernet address of the computer (virtual adapters and Wi‑Fi Direct / hosted-network adapters are skipped) and follows network changes while connected.
- **Optional username and password.** Off by default, as on Android. When on, SOCKS5 (RFC 1929) and the HTTP proxy (Basic) require it. A strong password is generated; you can set your own username and password (at least 8 characters) or **Generate a new password**. Both are sealed with DPAPI and never shown on the share pages.
- **`http://aether.check/` from every path.** Through the HTTP proxy, SOCKS5 or the PAC file, a shared device can open this address to see whether it is protected, which path it uses and the tunnel's exit IP.
- **Setup page.** `http://<computer address>:10811/` gives bilingual (Persian/English) instructions, the PAC file (`/proxy.pac`, `/wpad.dat`), a live WebRTC leak test and one-click WebRTC locks for Windows, Firefox, macOS and Linux. The pages load nothing from the internet.
- **Zero-leak guard.** Shared devices cannot reach private or local addresses of the host's network, never receive *Direct* (bypass) rules, identifying proxy headers are removed, and UDP through SOCKS5 is offered only when the engine can carry it.

### Core, routing and settings (carried over in 1.3.0)

- **Aether Core 2.3.0, unmodified.** All Tor modes use the bundled `tor.exe`; the protocol tile shows `Tor → Psiphon` instead of `SMART`. The Psiphon child is built from the compatible CluvexStudio commit `83aa73b9b982e7421e00117f5b0c5aceb5dda452` and receives the bundled server entries through `AETHER_PSIPHON_SERVER_ENTRIES`.
- **DNS and routing.** *Bypass Iranian sites* and *Block ads and trackers* are independent opt-in switches with bundled lists and a weekly refresh through the tunnel. Iranian hosts outside `.ir` are checked with DNS-over-HTTPS through the tunnel before going direct. In `Aether → Psiphon` and `Tor → Psiphon`, rules are applied at the local bridge.
- **A working server is no longer thrown away.** The app used to tell the engine to reuse the remembered server only if it answered faster than about **300 ms** — and on the networks reports actually came from, nothing answers that fast, so the remembered server was discarded every time and each reconnect paid for a full scan. That check is now **off by default**, so a remembered server that answers is used, as the engine always did on its own. If you are on a fast connection and want the app to look for the best server, the switch *Only reuse a fast endpoint* is still under **Transport & anti-DPI** — with limits that now match real networks (2000 ms / 5000 ms direct, 1200 ms / 3500 ms chained).
- **Your own ECH setting.** If you have an `ECHConfigList`, you can paste it under **Transport & anti-DPI** instead of letting the app fetch one — useful where your network blocks exactly that default fetch. Pointing ECH at a different domain, as v2rayNG does, is not possible: the engine does not accept a domain there.
- **Custom MTU** in the range **1280–9000**.
- **AI and interface.** The default model prefers **Gemini 3.1 Flash-Lite** when available; token limits are shown only when Google reports them. The connection card groups status, IP, time and traffic; settings use grouped cards and horizontal option tiles.
- **Saved backend is kept.** `Aether` is only the default for a new profile; restarting never replaces the user's choice.

### Packaging

Outputs are **x64/x86 `.exe` installers** and **x64/x86 portable `.zip` archives** plus `SHA256SUMS.txt`; no `.msi`. The version is **1.3.0** in every manifest, and the CI preflight checks this. Real tunnel and network behaviour still needs testing on Windows.

---

<div dir="rtl" align="right">
<h2 dir="rtl" align="right">‏تازه‌های نسخهٔ ویندوز ۱.۳.۰</h2>
<p dir="rtl" align="right">‏نسخهٔ برنامه همچنان <bdi>1.3.0</bdi> است. این انتشار سه قابلیت را از <bdi>Aether</bdi> اندروید <bdi>1.5.0</bdi> به ویندوز می‌آورد — <strong>کانفیگ‌های سفارشی</strong>، <strong>ورود <bdi>Zero Trust</bdi></strong> و <strong>اشتراک بازطراحی‌شده روی شبکهٔ محلی</strong> — با همان رفتار و همان متن‌های گوشی، و هستهٔ <strong>دست‌نخوردهٔ <bdi>Aether Core 2.3.0</bdi></strong> را همراه دارد. حالت «هات‌اسپات گوشی» اندروید عمداً در نسخهٔ ویندوز <strong>نیست</strong>. پروفایل‌های ذخیره‌شده و بک‌اند انتخابی حفظ می‌شوند.</p>
<h3 dir="rtl" align="right">‏کانفیگ‌های سفارشی (تازه، به‌طور پیش‌فرض خاموش)</h3>
<p dir="rtl" align="right">‏از <bdi>Aether</bdi> اندروید <bdi>1.5.0</bdi> آمده است: تنظیمات ← <strong>کانفیگ‌های سفارشی</strong>. تا وقتی کلید خاموش است (پیش‌فرض) هیچ چیز عوض نمی‌شود و شش حالت قبلی، پروفایل‌ها و موتور دقیقاً مثل قبل کار می‌کنند.</p>
<ul dir="rtl" align="right">
<li>‏<strong>چه چیزی وارد می‌شود:</strong> لینک‌های <bdi>vless://</bdi>، <bdi>vmess://</bdi>، <bdi>trojan://</bdi>، <bdi>ss://</bdi>، <bdi>socks://</bdi>، <bdi>wireguard://</bdi>؛ کانفیگ کامل <bdi>Xray JSON</bdi>؛ متن اشتراک (<bdi>Base64</bdi> یا ساده)؛ نشانی اشتراک (فقط <bdi>https://</bdi>)؛ و فایل‌های <bdi>.npvt</bdi>. اسکن <bdi>QR</bdi> در ویندوز نیست.</li>
<li>‏<strong>قفل‌های <bdi>.npvt</bdi> رعایت می‌شوند:</strong> کانفیگِ قفل‌شده به دستگاه دیگر، منقضی یا «فقط موبایل» با دلیل روشن رد می‌شود و لینک خامِ کانفیگ قفل‌شده هرگز نمایش، کپی یا ویرایش نمی‌شود.</li>
<li>‏<strong>فهرست ذخیره‌شده:</strong> انتخاب، تغییر نام، ویرایش (فقط بازها)، کپی لینک، حذف و «حذف همهٔ کانفیگ‌ها»؛ فهرست با <bdi>Windows DPAPI</bdi> رمز می‌شود.</li>
<li>‏<strong>۱۵ حالت زنجیره:</strong> کانفیگ تنها، جلوی موتورها یا پشت آن‌ها — از <bdi>Npvt</bdi> تنها تا <bdi>Tor → Aether → Npvt</bdi>.</li>
<li>‏<strong>هستهٔ <bdi>Xray v25.12.8</bdi></strong> با <bdi>SHA-256</bdi> پین شده است، مثل اندروید.</li>
</ul>
<h3 dir="rtl" align="right">‏کانفیگ‌های سفارشی: رفع زنجیره‌ها، برچسب‌ها و سرعت (همین بیلد)</h3>
<p dir="rtl" align="right">‏مرجع، <bdi>Aether</bdi> اندروید <bdi>1.5.0</bdi> با همان روش است. زنجیره‌های سالم (<bdi>Config</bdi>، <bdi>Config → Aether</bdi>، <bdi>Aether → Config</bdi>، <bdi>Tor → Config</bdi>، <bdi>Config → Psiphon</bdi> و شش حالت معمولی) دست نخورده‌اند.</p>
<ul dir="rtl" align="right">
<li>‏<strong>ریشهٔ کندی <bdi>Config → Tor</bdi>، <bdi>Config → Tor → Aether</bdi> و <bdi>Config → Tor → Psiphon</bdi> رفع شد:</strong> <bdi>tor.exe</bdi> در هر دست‌دادن با رله یک <bdi>SNI</bdi> ساختگی (<bdi>www.&lt;random&gt;.com</bdi>) می‌فرستد. بیشتر سرورهای کانفیگ (پیش‌فرض <bdi>3x-ui</bdi> و <bdi>Marzban</bdi>: <bdi>sniffing</bdi> با <bdi>destOverride: tls</bdi>) آن نام را می‌خوانند، آی‌پی رله را با دامنه‌ای که وجود ندارد عوض می‌کنند و اتصال را می‌بندند؛ نتیجه <bdi>TLS_ERROR</bdi> و ماندنِ تور روی ۱۰٪ بود. اندروید با <bdi>Arti</bdi> وصله‌شده اصلاً <bdi>SNI</bdi> نمی‌فرستد. ویندوز حالا معادلش را داخل <bdi>Xray</bdi> انجام می‌دهد: هاپِ حامل تور (<bdi>tor-link</bdi>) <bdi>ClientHello</bdi> را پیش از ورود به کانفیگ به رکوردهای ۱۰۰ تا ۲۰۰ بایتی می‌شکند تا sniffer سرور نامی نخواند و آدرس واقعی رله بماند. با دو کانفیگ نمونهٔ <bdi>.npvt</bdi> آزموده شد: <strong>بوت‌استرپ ۱۰۰٪ در ۹ تا ۱۲ ثانیه</strong> و <bdi>IsTor: true</bdi>، در حالی که قبلاً روی ۱۰٪ می‌ماند.</li>
<li>‏<strong>بدون انتظار پل پشت کانفیگ:</strong> این زنجیره‌ها «تلاش با پل» می‌نوشتند و ۴۲۰ ثانیه منتظر پل می‌ماندند، در حالی که وقتی تور از دل کانفیگ بیرون می‌رود پلی به کار نمی‌رود. حالا مثل اندروید پنجرهٔ عادی دارند.</li>
<li>‏<strong>شروع سریع‌تر تور:</strong> تورِ زنجیره‌ای دیگر فایل‌های <bdi>GeoIP</bdi> را پارس نمی‌کند (حدود ۷ ثانیه روی رایانهٔ گزارش‌دهنده پیش از «<bdi>Bootstrapped 0%</bdi>»).</li>
<li>‏<strong><bdi>Psiphon → Config</bdi>:</strong> برنامه هر ۲۰۰ میلی‌ثانیه پورت سایفون را باز و بسته می‌کرد و سایفون ثانیه‌ای پنج بار <bdi>socksPeekByte() failed: EOF</bdi> می‌نوشت. حالا پورت پس از اولین بار باز دیدن، باز حساب می‌شود. خطاهای اتصال مستقیم خود سایفون (<bdi>i/o timeout</bdi>) از شبکه است، نه از برنامه.</li>
<li>‏<strong>نام پروتکل به‌جای «کانفیگ»:</strong> پس از وارد کردن، فهرست زنجیره، ردیف تنظیمات و کارت اتصال نام واقعی پروتکل را نشان می‌دهند — <bdi>VLESS → Tor</bdi>، <bdi>Npvt → Aether</bdi>، <bdi>Tor → Aether(MASQUE) → Npvt</bdi> — در هر دو زبان به انگلیسی. فایل‌های <bdi>.npvt</bdi> با <bdi>Npvt</bdi> و بقیه با پروتکل خودشان (<bdi>VLESS</bdi>، <bdi>VMess</bdi>، <bdi>Trojan</bdi>، <bdi>SS</bdi>، <bdi>SOCKS</bdi>، <bdi>WireGuard</bdi>، <bdi>Hysteria</bdi>) نشان داده می‌شوند.</li>
<li>‏<strong>طراحی تازهٔ صفحهٔ کانفیگ‌های سفارشی مثل اندروید:</strong> منوی کشویی ویندوز جای خود را به ردیف «زنجیره» داد که برگه‌ای از کارت‌های گرد باز می‌کند: زنجیره با قلم درشت، زیرش «تکی · فقط کانفیگ» / «زنجیرهٔ دوتایی» / «زنجیرهٔ سه‌تایی» و کارت انتخاب‌شده با قاب آبی و تیک (صفحه‌کلید: جهت‌ها، <bdi>Enter</bdi>، <bdi>Esc</bdi>). کانفیگ‌های ذخیره‌شده هم کارت‌های گرد جدا هستند و انتخاب‌شده قاب آبی و تیک دارد.</li>
<li>‏<strong>قطع فوری:</strong> حذف قواعد فایروال و بازگرداندن سیاست‌ها حالا هم‌زمان انجام می‌شود، نه یک فرمان <bdi>netsh</bdi> پس از دیگری؛ پاک‌سازی قواعد جامانده پیش از اتصال هم همین‌طور.</li>
<li>‏<strong>مصرف کمتر پردازنده، حافظه و دیسک:</strong> خطی که پشت سر هم تکرار شود دو بار نوشته و بعد فقط شمرده می‌شود — <bdi>(previous line repeated N more times)</bdi> — و هر دسته از لاگ با یک فراخوانی روی دیسک می‌رود.</li>
</ul>
<h3 dir="rtl" align="right">‏ممیزی امنیتی — ویندوز ۱.۳.۰ در مقایسه با اندروید ۱.۵.۰</h3>
<p dir="rtl" align="right">‏بازبینی سطح سورس همین مخزن با همان ده حوزه و وزن‌های ممیزی اندروید <bdi>1.5.0</bdi>، به‌همراه آزمون‌های همین بیلد: <bdi>399</bdi> آزمون واحد <bdi>Rust</bdi> روی هدف ویندوز (اجرا با <bdi>Wine</bdi>)، آزمون‌های رابط کاربری، <bdi>cargo check</bdi> برای ویندوز و بوت‌استرپ واقعی تور از دل کانفیگ‌های نمونه. این آزمون نفوذ نیست و آزمون نشتی روی سخت‌افزار ویندوز اجرا نشد.</p>
<table dir="rtl" align="right">
<thead><tr><th align="right">‏#</th><th align="right">‏حوزه</th><th align="right">‏وزن</th><th align="right">‏ویندوز ۱.۳.۰</th><th align="right">‏اندروید ۱.۵.۰</th><th align="right">‏توضیح (ویندوز)</th></tr></thead>
<tbody>
<tr><td align="right">‏۱</td><td align="right">‏اسرار و مدیریت کلید</td><td align="right">‏۱۴</td><td align="right">‏۹۴</td><td align="right">‏۹۵</td><td align="right">‏کلید هوش مصنوعی، ورود <bdi>Zero Trust</bdi>، رمز اشتراک و کانفیگ‌ها با <bdi>DPAPI</bdi> رمز می‌شوند؛ هیچ رمزی در کد نیست. فایل‌های هویت <bdi>WARP</bdi> رمز نشده‌اند.</td></tr>
<tr><td align="right">‏۲</td><td align="right">‏رمزنگاری، <bdi>TLS</bdi> و مقاومت در برابر <bdi>MitM</bdi></td><td align="right">‏۱۳</td><td align="right">‏۹۵</td><td align="right">‏۹۶</td><td align="right">‏پین <bdi>SPKI</bdi> برقرار است و عیب‌یابی گواهی <strong>و</strong> نام میزبان را می‌سنجد. <bdi>allowInsecure</bdi> خودِ کانفیگ مثل اندروید رعایت می‌شود.</td></tr>
<tr><td align="right">‏۳</td><td align="right">‏خطر نشت داده (<bdi>DNS</bdi>، <bdi>IPv6</bdi>، <bdi>WebRTC</bdi>، دور زدن تونل)</td><td align="right">‏۱۷</td><td align="right">‏۸۹</td><td align="right">‏۹۰</td><td align="right">‏<bdi>DNS</bdi> داخل تونل، قفل <bdi>WebRTC</bdi>، <bdi>IPv6</bdi> بسته در صورت خطا و کلید قطع اضطراری. پراکسی سیستم برای هر برنامهٔ <bdi>UDP</bdi> مسیر همگانی نیست. هاپ تازهٔ <bdi>tor-link</bdi> داخل کانفیگ می‌ماند و خروجی تازه‌ای ندارد.</td></tr>
<tr><td align="right">‏۴</td><td align="right">‏سطح اشتراک روی شبکهٔ محلی</td><td align="right">‏۱۰</td><td align="right">‏۸۷</td><td align="right">‏۸۶</td><td align="right">‏نگهبان بدون نشتی، احراز هویت اختیاری، بدون دسترسی به نشانی‌های محلی و بدون قاعدهٔ مستقیم برای مهمان‌ها.</td></tr>
<tr><td align="right">‏۵</td><td align="right">‏ذخیره‌سازی محلی</td><td align="right">‏۱۲</td><td align="right">‏۸۸</td><td align="right">‏۹۳</td><td align="right">‏<bdi>profile.json</bdi>، لاگ چرخشی و هویت <bdi>WARP</bdi> رمز نشده‌اند؛ فایل کانفیگ <bdi>Xray</bdi> فقط هنگام اجرا وجود دارد.</td></tr>
<tr><td align="right">‏۶</td><td align="right">‏مجوزها و پیکربندی سیستم</td><td align="right">‏۷</td><td align="right">‏۹۲</td><td align="right">‏۹۷</td><td align="right">‏دسترسی مدیر لازم است (فایروال، سیاست <bdi>WebRTC</bdi>)؛ قواعد محدودند، هنگام قطع حذف و قواعد جامانده پاک می‌شوند.</td></tr>
<tr><td align="right">‏۷</td><td align="right">‏لاگ و عیب‌یابی</td><td align="right">‏۷</td><td align="right">‏۹۶</td><td align="right">‏۹۷</td><td align="right">‏آی‌پی عمومی، نام میزبان و <bdi>UUID</bdi> پوشانده می‌شوند؛ سرور <bdi>.npvt</bdi> قفل‌شده هرگز در لاگ نمی‌آید؛ خطوط تکراری جمع می‌شوند.</td></tr>
<tr><td align="right">‏۸</td><td align="right">‏کیفیت کد و پیکربندی شبکه</td><td align="right">‏۸</td><td align="right">‏۹۰</td><td align="right">‏۸۸</td><td align="right">‏شنونده‌ها فقط روی <bdi>loopback</bdi>؛ شروع دوباره محدود؛ زنجیره‌های تور از ریشه درست شدند و آزمون واحد دارند. <bdi>SOCKS</bdi> محلی زنجیره بی‌رمز است (<bdi>SB-5</bdi> اندروید، در هر دو باز).</td></tr>
<tr><td align="right">‏۹</td><td align="right">‏افشا روی خود دستگاه (صفحه، کلیپ‌بورد)</td><td align="right">‏۶</td><td align="right">‏۹۲</td><td align="right">‏۹۵</td><td align="right">‏لینک کانفیگ قفل‌شده کپی یا نمایش داده نمی‌شود؛ لینک بازی که کاربر کپی کند در کلیپ‌بورد می‌ماند.</td></tr>
<tr><td align="right">‏۱۰</td><td align="right">‏زنجیرهٔ تأمین و سلامت بیلد</td><td align="right">‏۶</td><td align="right">‏۸۲</td><td align="right">‏۸۰</td><td align="right">‏فایل‌های <bdi>Xray</bdi> با <bdi>SHA-256</bdi> پین شده‌اند و سلامت هسته بررسی می‌شود؛ بدون گواهی در <bdi>CI</bdi> نصب‌کننده امضای <bdi>Authenticode</bdi> ندارد.</td></tr>
<tr><td align="right"></td><td align="right">‏<strong>جمع</strong></td><td align="right">‏<strong>۱۰۰</strong></td><td align="right">‏<strong><bdi>90.70</bdi> ≈ ۹۱</strong></td><td align="right">‏<strong><bdi>91.96</bdi> ≈ ۹۲</strong></td><td align="right"></td></tr>
</tbody>
</table>
<p dir="rtl" align="right">‏یک امتیاز فاصله از طراحی خاص ویندوز می‌آید (وضعیت غیرمحرمانهٔ رمزنشده، دسترسی مدیر، پراکسی سیستم به‌جای مسیر <bdi>VpnService</bdi>)، نه از یک آسیب‌پذیری باز شناخته‌شده. هیچ تغییری در این بیلد آنچه از رایانه بیرون می‌رود را گسترده‌تر نمی‌کند: رفع تور فقط شکل شکستن <bdi>ClientHello</bdi> را داخل تونلِ خودِ کانفیگ عوض می‌کند.</p>
<h3 dir="rtl" align="right">‏ورود <bdi>Zero Trust</bdi> پیش از اتصال</h3>
<ul dir="rtl" align="right">
<li>‏<strong>اول وارد شوید:</strong> در تنظیمات ← <bdi>Zero Trust</bdi> ← «کد ایمیلی»، دکمهٔ «ورود» پنجره‌ای باز می‌کند: کد به ایمیل فرستاده می‌شود، آن را وارد می‌کنید (سه بار فرصت)، پس از ۳۰ ثانیه «ارسال کد تازه» فعال می‌شود و هر مشکل (تیم یا ایمیل نادرست، کد اشتباه یا منقضی، نبود اینترنت) به زبان ساده توضیح داده می‌شود.</li>
<li>‏<strong>رمزشده و قابل مشاهده:</strong> ورود با <bdi>Windows DPAPI</bdi> رمز می‌شود، به تیم <strong>و</strong> ایمیل گره می‌خورد و پنل نشان می‌دهد هنوز معتبر است یا نه و تا کی. «فراموش کردن این ورود» آن را پاک می‌کند. توکن هرگز به رابط وب نمی‌رسد.</li>
<li>‏<strong>کد هنگام اتصال:</strong> اگر موتور وسط اتصال کد ایمیلی بخواهد، پنجره‌ای روی هر صفحه‌ای باز می‌شود. «حالا نه» همان تلاش اتصال را متوقف می‌کند و آن را معطل نمی‌گذارد.</li>
<li>‏<strong>عضویت:</strong> دستگاه عضوشده وضعیت عضویتش را می‌بیند و با «حذف عضویت این دستگاه» (با تأیید و فقط وقتی قطع است) آن را برمی‌دارد؛ اتصال بعدی دوباره عضو می‌شود.</li>
<li>‏<strong>توکن سرویس و توکن عضویت:</strong> در «توکن سرویس»، دکمهٔ «آزمایش توکن سرویس» همان مبادلهٔ موتور را بدون ذخیرهٔ چیزی اجرا می‌کند. «توکن دسترسی» (توکن عضویت) چسبانده‌شده همان لحظه بررسی می‌شود (خالی، نامعتبر، منقضی یا معتبر با زمان باقی‌مانده).</li>
</ul>
<h3 dir="rtl" align="right">‏اشتراک <bdi>VPN</bdi> روی شبکهٔ محلی</h3>
<ul dir="rtl" align="right">
<li>‏<strong>صفحهٔ «اشتراک» بازطراحی شد:</strong> وضعیت زنده («خاموش»، «در انتظار شبکه»، «آماده»، «پورت اشغال است»)، کلید «اشتراک‌گذاری در این شبکه»، نشان «بدون نشتی»، ردیف‌های قابل کپی <bdi>SOCKS5 10810</bdi>، <bdi>HTTP 10811</bdi> و آدرس، نشانی‌های دیگر رایانه، «کپی همهٔ مشخصات»، مراحل راه‌اندازی و نشانی بررسی.</li>
<li>‏<strong>همهٔ آداپتورهای واقعی:</strong> اشتراک روی همهٔ نشانی‌های <bdi>Wi‑Fi</bdi> و <bdi>Ethernet</bdi> رایانه گوش می‌دهد (آداپتورهای مجازی و <bdi>Wi‑Fi Direct</bdi> کنار گذاشته می‌شوند) و هنگام اتصال، تغییر شبکه را دنبال می‌کند.</li>
<li>‏<strong>نام کاربری و رمز اختیاری:</strong> مثل اندروید، به‌طور پیش‌فرض خاموش است. وقتی روشن باشد، <bdi>SOCKS5</bdi> (<bdi>RFC 1929</bdi>) و پراکسی <bdi>HTTP</bdi> (<bdi>Basic</bdi>) آن را می‌خواهند. رمزی قوی ساخته می‌شود؛ می‌توانید نام کاربری و رمز دلخواه (دست‌کم ۸ نویسه) بگذارید یا با «ساخت رمز جدید» آن را عوض کنید. هر دو با <bdi>DPAPI</bdi> رمز می‌شوند و هرگز در صفحه‌های اشتراک نمایش داده نمی‌شوند.</li>
<li>‏<strong><bdi>http://aether.check/</bdi> از هر مسیر:</strong> دستگاه مهمان از راه پراکسی <bdi>HTTP</bdi>، <bdi>SOCKS5</bdi> یا فایل <bdi>PAC</bdi> با باز کردن این نشانی می‌بیند محافظت می‌شود یا نه، از کدام مسیر وصل است و آی‌پی خروجی تونل چیست.</li>
<li>‏<strong>صفحهٔ راه‌اندازی:</strong> نشانی <bdi>http://&lt;نشانی رایانه&gt;:10811/</bdi> راهنمای دوزبانه (فارسی و انگلیسی)، فایل <bdi>PAC</bdi> (<bdi>/proxy.pac</bdi> و <bdi>/wpad.dat</bdi>)، آزمون زندهٔ نشت <bdi>WebRTC</bdi> و قفل‌های یک‌کلیکی <bdi>WebRTC</bdi> برای <bdi>Windows</bdi>، <bdi>Firefox</bdi>، <bdi>macOS</bdi> و <bdi>Linux</bdi> را دارد. این صفحه‌ها هیچ چیزی از اینترنت بارگذاری نمی‌کنند.</li>
<li>‏<strong>نگهبان بدون نشتی:</strong> دستگاه‌های مهمان به نشانی‌های خصوصی یا محلی شبکهٔ میزبان دسترسی ندارند، هیچ قاعدهٔ «مستقیم» (بای‌پس) نمی‌گیرند، سرآیندهای شناسایی‌کنندهٔ پراکسی حذف می‌شوند و <bdi>UDP</bdi> از راه <bdi>SOCKS5</bdi> فقط وقتی ارائه می‌شود که موتور بتواند آن را حمل کند.</li>
</ul>
<h3 dir="rtl" align="right">‏هسته، مسیریابی و تنظیمات (از پیش در ۱.۳.۰)</h3>
<ul dir="rtl" align="right">
<li>‏<strong>هستهٔ دست‌نخوردهٔ <bdi>Aether Core 2.3.0</bdi>:</strong> همهٔ حالت‌های تور از <bdi>tor.exe</bdi> همراه برنامه استفاده می‌کنند و کاشی پروتکل به‌جای <bdi>SMART</bdi> عنوان <bdi>Tor → Psiphon</bdi> را نشان می‌دهد. فرزند سایفون از کامیت سازگار <bdi>CluvexStudio</bdi> ساخته می‌شود و سرورهای همراه را از راه <bdi>AETHER_PSIPHON_SERVER_ENTRIES</bdi> می‌گیرد.</li>
<li>‏<strong><bdi>DNS</bdi> و مسیریابی:</strong> «بای‌پس سایت‌های ایرانی» و «حذف تبلیغات و ردیاب‌ها» دو کلید اختیاری و مستقل‌اند، با فهرست‌های همراه و به‌روزرسانی هفتگی از داخل تونل. دامنه‌های ایرانیِ غیر از <bdi>.ir</bdi> پیش از مستقیم شدن با <bdi>DNS-over-HTTPS</bdi> از داخل تونل بررسی می‌شوند. در <bdi>Aether → Psiphon</bdi> و <bdi>Tor → Psiphon</bdi> قواعد در پل محلی اعمال می‌شوند.</li>
<li>‏<strong>سروری که کار می‌کند دیگر دور ریخته نمی‌شود:</strong> برنامه به موتور می‌گفت سرورِ به‌خاطرسپرده را فقط در صورتی دوباره استفاده کن که سریع‌تر از حدود <strong>۳۰۰ میلی‌ثانیه</strong> جواب بدهد — و در شبکه‌هایی که واقعاً از آن‌ها گزارش رسیده، هیچ چیزی این‌قدر سریع جواب نمی‌دهد. پس سرورِ به‌خاطرسپرده هر بار دور ریخته می‌شد و هر اتصال مجدد هزینهٔ یک جست‌وجوی کامل را می‌پرداخت. آن بررسی حالا <strong>به‌طور پیش‌فرض خاموش</strong> است، پس سرورِ به‌خاطرسپرده‌ای که جواب می‌دهد، همان‌طور که موتور همیشه خودش عمل می‌کرد، استفاده می‌شود. اگر روی اتصال سریعی هستید و می‌خواهید برنامه دنبال بهترین سرور بگردد، سوئیچ «فقط اندپوینت سریع را دوباره استفاده کن» هنوز زیر «ترابرد و ضد <bdi>DPI</bdi>» هست — با حدودی که این بار با شبکه‌های واقعی می‌خواند (<bdi>2000/5000</bdi> میلی‌ثانیه مستقیم و <bdi>1200/3500</bdi> میلی‌ثانیه زنجیره‌ای).</li>
<li>‏<strong>تنظیم <bdi>ECH</bdi> خودتان:</strong> اگر یک <bdi>ECHConfigList</bdi> دارید، می‌توانید زیر «ترابرد و ضد <bdi>DPI</bdi>» بچسبانیدش به‌جای اینکه برنامه خودش یکی بگیرد — جایی به کار می‌آید که شبکه‌تان دقیقاً همان گرفتنِ پیش‌فرض را بلاک می‌کند. نشانه‌گرفتن <bdi>ECH</bdi> به یک دامنهٔ دیگر، آن‌طور که <bdi>v2rayNG</bdi> می‌کند، ممکن نیست: موتور در آن‌جا دامنه نمی‌پذیرد.</li>
<li>‏<strong><bdi>MTU</bdi> دلخواه</strong> در بازهٔ <strong>۱۲۸۰ تا ۹۰۰۰</strong>.</li>
<li>‏<strong>هوش مصنوعی و ظاهر:</strong> مدل پیش‌فرض در صورت دسترسی <bdi>Gemini 3.1 Flash-Lite</bdi> است و سقف توکن فقط وقتی نمایش داده می‌شود که گوگل اعلامش کند. کارت اتصال وضعیت، آی‌پی، زمان و ترافیک را یک‌جا نشان می‌دهد و تنظیمات کارت‌های گروه‌بندی‌شده و کاشی‌های افقی دارند.</li>
<li>‏<strong>حفظ بک‌اند انتخابی:</strong> <bdi>Aether</bdi> فقط پیش‌فرض پروفایل تازه است و بستن و باز کردن برنامه انتخاب کاربر را عوض نمی‌کند.</li>
</ul>
<h3 dir="rtl" align="right">‏بسته‌ها</h3>
<p dir="rtl" align="right">‏خروجی‌ها نصب‌کننده‌های <bdi>x64/x86</bdi> با پسوند <bdi>.exe</bdi> و بسته‌های پرتابل <bdi>.zip</bdi> به‌همراه <bdi>SHA256SUMS.txt</bdi> هستند؛ فایل <bdi>.msi</bdi> وجود ندارد. نسخه در همهٔ فایل‌های مشخصات <bdi>1.3.0</bdi> است و پیش‌بررسی <bdi>CI</bdi> این را کنترل می‌کند. رفتار واقعی تونل و شبکه همچنان باید روی ویندوز آزموده شود.</p>
</div>
