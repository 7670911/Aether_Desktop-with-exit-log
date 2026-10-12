//! ۱.۳.۰ — «پینگِ واقعی» برای کانفیگ‌های سفارشی؛ پورتِ `RealDelay.kt` و
//! `ConfigPing.kt` نسخهٔ اندروید ۱.۵.۰.
//!
//! پینگِ TCP به سرور هیچ نمی‌گوید: سرورِ REALITY/TLS پورتش را باز نگه می‌دارد
//! حتی وقتی UUID باطل شده یا مسیرِ آن‌سو بسته است. پس هر کانفیگ واقعاً در یک
//! Xrayِ موقت اجرا می‌شود و یک `GET /generate_204` از **دلِ همان کانفیگ**
//! فرستاده می‌شود؛ زمان از پاسخِ CONNECTِ SOCKS تا نخستین سطرِ وضعیتِ HTTP
//! است (Xray پیش از dialِ سرور به CONNECT پاسخ می‌دهد، پس دست‌دادنِ سرور و
//! رفت‌وبرگشتِ مقصد هر دو در عدد هستند).
//!
//! ```text
//!   یک xray.exe برای هر دسته (≤ ۳۲ کانفیگ)
//!   inbound  ping-in-<i>  SOCKS5 127.0.0.1:<p_i>  (رمزِ تصادفی، UDP خاموش، بی sniffing)
//!   outbound ping-out-<i> کانفیگِ i، مستقیم
//!   routing  ping-in-<i> → ping-out-<i>
//! ```
//!
//! دو دور، کمینه گزارش می‌شود؛ هر دور ۸ ثانیه مهلت دارد. اگر دسته بالا نیاید
//! (یک کانفیگِ خراب کلِ Xray را می‌کشد) هر کانفیگ تنها آزموده می‌شود و آن که
//! باز هم بالا نیاید «Invalid config» است. پس از شکست، تشخیصِ مستقیم (حلِ نام +
//! TCP، ۴ ثانیه) می‌گوید سرور خاموش است، فیلتر شده، یا پاسخ نمی‌دهد. در لاگ
//! هرگز نشانیِ سرور یا UUID نمی‌آید (`process::redact`).

use super::process::{self as xproc, XrayProcess};
use super::xray::direct_outbound;
use crate::log::DiagnosticsLog;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TAG: &str = "custom";
/// بیشینهٔ کانفیگ در یک Xrayِ موقت.
pub const BATCH: usize = 32;
/// مهلتِ هر دورِ سنجش.
pub const ROUND_TIMEOUT: Duration = Duration::from_secs(8);
pub const ROUNDS: usize = 2;
/// مهلتِ تشخیصِ مستقیم (حلِ نام و TCP).
const DIAG_TIMEOUT: Duration = Duration::from_secs(4);
/// مهلتِ بالا آمدنِ شنونده‌های دسته.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// مقصدهای سنجش: همان‌هایی که اندروید می‌زند.
pub const PROBE_HOSTS: [&str; 2] = ["www.gstatic.com", "cp.cloudflare.com"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// پاسخِ HTTP از دلِ کانفیگ رسید.
    Ok,
    /// سرور در دسترس است (TCP باز) ولی از دلِ کانفیگ پاسخی نیامد: −1.
    NoReply,
    /// نام حل نشد، یا اتصال رد شد / مقصد دست‌نیافتنی: سرور خاموش است.
    Offline,
    /// مهلت، reset، یا نامی که به sinkhole حل می‌شود: فیلتر شده.
    Blocked,
    /// Xray این کانفیگ را نپذیرفت.
    Invalid,
    /// این دستگاه اصلاً شبکه ندارد.
    NoNetwork,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Ok => "OK",
            Verdict::NoReply => "No response",
            Verdict::Offline => "Server off",
            Verdict::Blocked => "Filtered",
            Verdict::Invalid => "Invalid config",
            Verdict::NoNetwork => "No network",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delay {
    pub id: String,
    /// میلی‌ثانیه؛ −1 یعنی پاسخی نیامد.
    pub ms: i64,
    pub status: Verdict,
    pub label: &'static str,
}

impl Delay {
    fn new(id: &str, ms: i64, status: Verdict) -> Self {
        Delay { id: id.to_string(), ms, status, label: status.label() }
    }
}

/// یک کانفیگ برای سنجش.
#[derive(Debug, Clone)]
pub struct Target {
    pub id: String,
    pub name: String,
    pub outbound: String,
}

/// رمزِ تصادفی بی هیچ crateِ اضافه: `RandomState` هر بار کلیدِ تازه‌ای از سیستم‌عامل می‌گیرد.
fn random_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

struct Lane {
    index: usize,
    port: u16,
    user: String,
    pass: String,
}

/// کانفیگِ Xrayِ یک دسته. `None` برای هر کانفیگی که حتی JSONِ درستی نیست.
pub fn batch_config(outbounds: &[(usize, Value)], ports: &[u16], creds: &[(String, String)]) -> String {
    let mut inbounds = Vec::new();
    let mut outs = Vec::new();
    let mut rules = vec![json!({ "type": "field", "inboundTag": ["ping-dns"], "outboundTag": "direct" })];
    for (k, (i, o)) in outbounds.iter().enumerate() {
        let (user, pass) = &creds[k];
        inbounds.push(json!({
            "tag": format!("ping-in-{i}"),
            "listen": "127.0.0.1",
            "port": ports[k],
            "protocol": "socks",
            "settings": {
                "auth": "password",
                "accounts": [ { "user": user, "pass": pass } ],
                "udp": false,
                "userLevel": 0
            },
            "sniffing": { "enabled": false }
        }));
        let mut o = o.clone();
        o["tag"] = json!(format!("ping-out-{i}"));
        outs.push(o);
        rules.push(json!({ "type": "field", "inboundTag": [format!("ping-in-{i}")], "outboundTag": format!("ping-out-{i}") }));
    }
    outs.push(json!({ "tag": "direct", "protocol": "freedom", "settings": { "domainStrategy": "UseIP" } }));
    json!({
        "log": { "loglevel": "error", "access": "none" },
        "dns": { "tag": "ping-dns", "queryStrategy": "UseIP", "servers": ["1.1.1.1", "8.8.8.8", "localhost"] },
        "inbounds": inbounds,
        "outbounds": outs,
        "policy": { "levels": { "0": { "handshake": 8, "connIdle": 30, "uplinkOnly": 2, "downlinkOnly": 5 } } },
        "routing": { "domainStrategy": "AsIs", "rules": rules }
    })
    .to_string()
}

/// همهٔ `targets` را می‌سنجد. ترتیبِ خروجی همان ترتیبِ ورودی است.
pub fn measure(binary: &Path, data_dir: &Path, targets: &[Target]) -> Vec<Delay> {
    let mut results: Vec<Option<Delay>> = vec![None; targets.len()];
    let mut valid: Vec<(usize, Value)> = Vec::new();
    for (i, t) in targets.iter().enumerate() {
        match direct_outbound(&t.outbound, &format!("ping-out-{i}")) {
            Ok(o) => valid.push((i, o)),
            Err(_) => results[i] = Some(Delay::new(&t.id, -1, Verdict::Invalid)),
        }
    }
    for chunk in valid.chunks(BATCH) {
        match run_batch(binary, data_dir, targets, chunk) {
            Some(rows) => {
                for (i, d) in rows {
                    results[i] = Some(d);
                }
            }
            None if chunk.len() == 1 => {
                let i = chunk[0].0;
                results[i] = Some(Delay::new(&targets[i].id, -1, Verdict::Invalid));
            }
            None => {
                // یک کانفیگِ خراب کلِ دسته را کشت: تک‌تک.
                for one in chunk {
                    let i = one.0;
                    let d = run_batch(binary, data_dir, targets, std::slice::from_ref(one))
                        .and_then(|mut r| r.pop().map(|(_, d)| d))
                        .unwrap_or_else(|| Delay::new(&targets[i].id, -1, Verdict::Invalid));
                    results[i] = Some(d);
                }
            }
        }
    }
    let out: Vec<Delay> = results
        .into_iter()
        .enumerate()
        .map(|(i, d)| d.unwrap_or_else(|| Delay::new(&targets[i].id, -1, Verdict::Invalid)))
        .collect();
    for (t, d) in targets.iter().zip(out.iter()) {
        let name = xproc::redact(&t.name);
        let line = if d.ms >= 0 {
            format!("Real delay {name}: {} ms", d.ms)
        } else {
            format!("Real delay {name}: -1 -> {}", d.label)
        };
        DiagnosticsLog::i(TAG, &line);
    }
    out
}

/// `None` یعنی Xrayِ این دسته بالا نیامد.
fn run_batch(binary: &Path, data_dir: &Path, targets: &[Target], chunk: &[(usize, Value)]) -> Option<Vec<(usize, Delay)>> {
    let mut ports: Vec<u16> = Vec::with_capacity(chunk.len());
    for _ in chunk {
        let p = xproc::pick_port(&ports);
        if ports.contains(&p) {
            return None;
        }
        ports.push(p);
    }
    let creds: Vec<(String, String)> = chunk.iter().map(|_| (random_token()[..12].to_string(), random_token())).collect();
    let config = batch_config(chunk, &ports, &creds);
    let tag = format!("ping-{}", ports[0]);
    let mut x = XrayProcess::start(binary, data_dir, &tag, ports[0], &config).ok()?;
    let started = Instant::now();
    loop {
        if !x.is_alive() {
            return None;
        }
        if ports.iter().all(|p| port_open(*p)) {
            break;
        }
        if started.elapsed() > START_TIMEOUT {
            x.stop();
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let lanes: Vec<Lane> = chunk
        .iter()
        .enumerate()
        .map(|(k, (i, _))| Lane { index: *i, port: ports[k], user: creds[k].0.clone(), pass: creds[k].1.clone() })
        .collect();
    let (tx, rx) = mpsc::channel::<(usize, i64)>();
    for lane in lanes {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("aether-realdelay".into())
            .spawn(move || {
                let mut best: i64 = -1;
                for _ in 0..ROUNDS {
                    if let Some(ms) = probe_once(lane.port, &lane.user, &lane.pass) {
                        if best < 0 || ms < best {
                            best = ms;
                        }
                    }
                }
                let _ = tx.send((lane.index, best));
            })
            .ok()?;
    }
    drop(tx);
    let mut out = Vec::new();
    let deadline = Instant::now() + ROUND_TIMEOUT * (ROUNDS as u32 * PROBE_HOSTS.len() as u32) + Duration::from_secs(4);
    while out.len() < chunk.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((i, ms)) => out.push((i, ms)),
            Err(_) => break,
        }
    }
    x.stop();
    // اتصال‌ها بسته شدند؛ حالا تشخیصِ مستقیم برای آن‌هایی که پاسخ ندادند (موازی).
    let mut rows: Vec<(usize, Delay)> = Vec::new();
    let mut diag = Vec::new();
    for (i, _) in chunk {
        let ms = out.iter().find(|(j, _)| j == i).map(|(_, ms)| *ms).unwrap_or(-1);
        if ms >= 0 {
            rows.push((*i, Delay::new(&targets[*i].id, ms, Verdict::Ok)));
        } else {
            let outbound = targets[*i].outbound.clone();
            let i = *i;
            diag.push((i, std::thread::spawn(move || diagnose(&outbound))));
        }
    }
    for (i, h) in diag {
        let v = h.join().unwrap_or(Verdict::NoReply);
        rows.push((i, Delay::new(&targets[i].id, -1, v)));
    }
    Some(rows)
}

fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(300)).is_ok()
}

/// یک دور: SOCKS5 با رمز → CONNECT → `GET /generate_204`. میلی‌ثانیه از پاسخِ
/// CONNECT تا نخستین سطرِ وضعیت.
pub fn probe_once(port: u16, user: &str, pass: &str) -> Option<i64> {
    for host in PROBE_HOSTS {
        if let Some(ms) = probe_host(port, user, pass, host) {
            return Some(ms);
        }
    }
    None
}

fn probe_host(port: u16, user: &str, pass: &str, host: &str) -> Option<i64> {
    let deadline = Instant::now() + ROUND_TIMEOUT;
    let mut s = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(2)).ok()?;
    let _ = s.set_nodelay(true);
    let left = |d: Instant| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
    s.set_read_timeout(Some(left(deadline))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    // greeting: فقط username/password.
    s.write_all(&[5, 1, 2]).ok()?;
    let mut b2 = [0u8; 2];
    s.read_exact(&mut b2).ok()?;
    if b2 != [5, 2] {
        return None;
    }
    let mut auth = vec![1u8, user.len() as u8];
    auth.extend_from_slice(user.as_bytes());
    auth.push(pass.len() as u8);
    auth.extend_from_slice(pass.as_bytes());
    s.write_all(&auth).ok()?;
    s.read_exact(&mut b2).ok()?;
    if b2[1] != 0 {
        return None;
    }
    let mut req = vec![5u8, 1, 0, 3, host.len() as u8];
    req.extend_from_slice(host.as_bytes());
    req.extend_from_slice(&80u16.to_be_bytes());
    s.write_all(&req).ok()?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).ok()?;
    if head[1] != 0 {
        return None;
    }
    let skip = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).ok()?;
            l[0] as usize + 2
        }
        _ => return None,
    };
    let mut rest = vec![0u8; skip];
    s.read_exact(&mut rest).ok()?;
    let t0 = Instant::now();
    let get = format!(
        "GET /generate_204 HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    s.write_all(get.as_bytes()).ok()?;
    s.set_read_timeout(Some(left(deadline))).ok()?;
    let mut buf = [0u8; 16];
    let mut got = 0;
    while got < 12 {
        let n = s.read(&mut buf[got..]).ok()?;
        if n == 0 {
            break;
        }
        got += n;
    }
    if got >= 12 && buf.starts_with(b"HTTP/1.") && buf[9].is_ascii_digit() {
        Some(t0.elapsed().as_millis().max(1) as i64)
    } else {
        None
    }
}

/// نشانی‌ای که یک DNSِ مسموم به‌جای سرورِ واقعی برمی‌گرداند.
pub fn is_sinkhole(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_unspecified()
                || o[0] == 127
                || o[0] == 10
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 169 && o[1] == 254)
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || o[0] >= 224
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            v.is_unspecified()
                || v.is_loopback()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || s[0] == 0x00d0
        }
    }
}

/// پس از شکست: سرور خاموش است، فیلتر شده، یا باز است و فقط پاسخ نداد؟
pub fn diagnose(outbound: &str) -> Verdict {
    let Ok(Value::Object(o)) = serde_json::from_str::<Value>(outbound) else {
        return Verdict::Invalid;
    };
    let protocol = o.get("protocol").and_then(|p| p.as_str()).unwrap_or("").to_ascii_lowercase();
    let (host, port) = super::parser::server_of(&o);
    if host.is_empty() || !(1..=65535).contains(&port) {
        return Verdict::Invalid;
    }
    let host = host.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = port as u16;
    // حلِ نام با مهلت (ToSocketAddrs خودش مهلت ندارد).
    let (tx, rx) = mpsc::channel();
    let h = host.clone();
    std::thread::spawn(move || {
        let r = (h.as_str(), port).to_socket_addrs().map(|a| a.collect::<Vec<_>>());
        let _ = tx.send(r);
    });
    let addrs = match rx.recv_timeout(DIAG_TIMEOUT) {
        Ok(Ok(a)) if !a.is_empty() => a,
        Ok(_) => return if network_up() { Verdict::Offline } else { Verdict::NoNetwork },
        Err(_) => return if network_up() { Verdict::Blocked } else { Verdict::NoNetwork },
    };
    if host.parse::<IpAddr>().is_err() && addrs.iter().all(|a| is_sinkhole(a.ip())) {
        return Verdict::Blocked;
    }
    // سرورهای UDP (WireGuard، Hysteria): TCP چیزی نمی‌گوید.
    if matches!(protocol.as_str(), "wireguard" | "hysteria" | "hysteria2") {
        return Verdict::NoReply;
    }
    let addr = addrs.iter().find(|a| a.is_ipv4()).unwrap_or(&addrs[0]);
    match TcpStream::connect_timeout(addr, DIAG_TIMEOUT) {
        Ok(_) => Verdict::NoReply,
        Err(e) => match e.kind() {
            ErrorKind::ConnectionRefused => Verdict::Offline,
            ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
                if network_up() {
                    Verdict::Blocked
                } else {
                    Verdict::NoNetwork
                }
            }
            _ => {
                // «network/host unreachable» و مانندش.
                if network_up() {
                    Verdict::Offline
                } else {
                    Verdict::NoNetwork
                }
            }
        },
    }
}

/// آیا اصلاً شبکه‌ای هست؟ دو IP، بی DNS.
fn network_up() -> bool {
    [([1, 1, 1, 1], 443u16), ([8, 8, 8, 8], 53u16)].iter().any(|(ip, p)| {
        TcpStream::connect_timeout(&SocketAddr::from((*ip, *p)), Duration::from_secs(2)).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::parser;
    use crate::custom::ConfigSource;

    fn vless() -> String {
        let link = "vless://aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee@203.0.113.9:443?security=tls&type=ws&path=%2Fws&host=cdn.example.org&sni=cdn.example.org#t";
        parser::parse(link, ConfigSource::Link, None, 0).unwrap()[0].outbound.clone()
    }

    #[test]
    fn labels_match_the_ui_contract() {
        assert_eq!(Verdict::NoReply.label(), "No response");
        assert_eq!(Verdict::Offline.label(), "Server off");
        assert_eq!(Verdict::Blocked.label(), "Filtered");
        assert_eq!(Verdict::Invalid.label(), "Invalid config");
        let d = serde_json::to_value(Delay::new("x", -1, Verdict::Blocked)).unwrap();
        assert_eq!(d["ms"], -1);
        assert_eq!(d["status"], "blocked");
        assert_eq!(d["label"], "Filtered");
    }

    #[test]
    fn batch_config_is_password_protected_and_routes_lane_by_lane() {
        let o = direct_outbound(&vless(), "x").unwrap();
        let cfg: Value = serde_json::from_str(&batch_config(
            &[(0, o.clone()), (3, o)],
            &[18831, 18832],
            &[("u1".into(), "p1".into()), ("u2".into(), "p2".into())],
        ))
        .unwrap();
        let ins = cfg["inbounds"].as_array().unwrap();
        assert_eq!(ins.len(), 2);
        for i in ins {
            assert_eq!(i["listen"], "127.0.0.1");
            assert_eq!(i["settings"]["auth"], "password");
            assert_eq!(i["settings"]["udp"], false);
            assert_eq!(i["sniffing"]["enabled"], false);
        }
        assert_eq!(cfg["outbounds"][1]["tag"], "ping-out-3");
        let rules = cfg["routing"]["rules"].as_array().unwrap();
        assert!(rules.iter().any(|r| r["inboundTag"][0] == "ping-in-3" && r["outboundTag"] == "ping-out-3"));
        assert!(cfg["outbounds"][0]["streamSettings"]["sockopt"].get("dialerProxy").is_none());
    }

    #[test]
    fn sinkholes_are_recognised() {
        for ip in ["0.0.0.0", "10.10.34.35", "127.0.0.1", "192.168.1.1", "172.20.0.1", "100.64.0.1", "224.0.0.1", "::", "fd00::1", "fe80::1"] {
            assert!(is_sinkhole(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["31.77.13.19", "1.1.1.1", "2606:4700::1111"] {
            assert!(!is_sinkhole(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn random_tokens_differ() {
        assert_ne!(random_token(), random_token());
        assert_eq!(random_token().len(), 32);
    }

    #[test]
    fn broken_outbounds_are_invalid_without_starting_xray() {
        let d = measure(
            Path::new("/nonexistent/xray"),
            &std::env::temp_dir(),
            &[Target { id: "a".into(), name: "a".into(), outbound: "nope".into() }],
        );
        assert_eq!(d[0].status, Verdict::Invalid);
        assert_eq!(d[0].ms, -1);
    }

    /// Optional: `XRAY_BIN=/path/to/xray AETHER_PING_LINKS=/path/v2ray.txt cargo test real_delay_live -- --nocapture`
    #[test]
    fn real_delay_live() {
        let (Ok(bin), Ok(file)) = (std::env::var("XRAY_BIN"), std::env::var("AETHER_PING_LINKS")) else {
            eprintln!("XRAY_BIN / AETHER_PING_LINKS not set — skipping the live real-delay check");
            return;
        };
        let text = std::fs::read_to_string(&file).unwrap();
        let configs = crate::custom::import::from_text(&text, None, crate::mini_https::Dial::Direct)
            .or_else(|_| crate::custom::import::from_file_text(&file, &text, None))
            .unwrap();
        let targets: Vec<Target> = configs
            .iter()
            .map(|c| Target { id: c.id.clone(), name: c.name.clone(), outbound: c.outbound.clone() })
            .collect();
        let dir = std::env::temp_dir().join("aether-ping-test");
        let out = measure(Path::new(&bin), &dir, &targets);
        for (t, d) in targets.iter().zip(out.iter()) {
            eprintln!("{} -> {} ms ({:?} / {})", xproc::redact(&t.name), d.ms, d.status, d.label);
        }
        assert_eq!(out.len(), targets.len());
    }
}
