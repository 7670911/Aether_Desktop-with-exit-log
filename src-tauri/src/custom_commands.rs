//! ۱.۳.۰ — فرمان‌های Tauri برای صفحهٔ «کانفیگ‌های سفارشی».
//!
//! نمایی که به رابط کاربری می‌رود **هرگز** outbound یا متنِ خامِ کانفیگ را
//! ندارد؛ متنِ خام فقط با `cc_get_raw` و فقط برای کانفیگِ قفل‌نشده داده
//! می‌شود (برای ویرایش و کپیِ لینک). کانفیگِ قفل‌شدهٔ npvt نشانی، کلید و
//! متنِ خامش را هیچ‌جا نشان نمی‌دهد.
//!
//! بدونِ QR: نسخهٔ ویندوز دوربین و اسکنرِ تصویر ندارد. ورود از راهِ متن،
//! نشانیِ اشتراکِ https، انتخابِ فایل (در خودِ صفحه) و کشیدن‌ورها‌کردنِ فایل.

use crate::custom::parser::ImportError;
use crate::custom::store::{self, State as CcState};
use crate::custom::{import, now_millis, realdelay, ChainMode, CustomConfig};
use crate::mini_https::Dial;
use crate::state::ConnectionState;
use crate::AppState;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcError {
    /// `password` (رمز لازم است)، `wrongPassword`، `invalid`، `locked`، `notFound`.
    pub kind: &'static str,
    pub message: String,
}

impl From<ImportError> for CcError {
    fn from(e: ImportError) -> Self {
        let kind = match e {
            ImportError::Password { wrong: false } => "password",
            ImportError::Password { wrong: true } => "wrongPassword",
            ImportError::Invalid(_) => "invalid",
        };
        CcError { kind, message: e.to_string() }
    }
}

fn not_found() -> CcError {
    CcError { kind: "notFound", message: "config not found".into() }
}

fn config_view(c: &CustomConfig, now: i64) -> Value {
    json!({
        "id": c.id,
        "name": c.name,
        "protocol": c.protocol.short(),
        "format": c.format.name(),
        "source": c.source.name(),
        // قفل‌شده: خالی (همان چیزی که انبار نگه می‌دارد).
        "server": if c.locked { String::new() } else { c.server.clone() },
        "locked": c.locked,
        "note": c.note,
        "expiresAt": c.expires_at,
        "expired": c.is_expired(now),
        "addedAt": c.added_at,
        "chainLabel": c.chain_label(),
        "shareable": !c.locked && !c.raw.is_empty(),
    })
}

pub fn view(s: &CcState) -> Value {
    let now = now_millis();
    let active = s.active();
    // نامِ پروتکلِ کانفیگِ انتخاب‌شده (VLESS، Npvt، VMess…) جای «Config»ِ عمومی
    // می‌نشیند — همان `chainLabel` موبایل ۱.۵.۰. فقط بی‌کانفیگ «Config» می‌ماند.
    let name = active.map(|c| c.chain_label()).unwrap_or("Config");
    let modes: Vec<Value> = ChainMode::ALL
        .iter()
        .map(|m| json!({ "id": m.name(), "label": m.label(name, None), "hops": m.hops() }))
        .collect();
    json!({
        "enabled": s.enabled,
        "mode": s.mode.name(),
        "modeLabel": s.mode.label(name, None),
        "activeId": s.active_id,
        "modes": modes,
        "configs": s.configs.iter().map(|c| config_view(c, now)).collect::<Vec<_>>(),
        "max": store::MAX_CONFIGS,
        "qr": false,
    })
}

fn publish(app: &AppHandle, s: &CcState) -> Value {
    let v = view(s);
    let _ = app.emit("aether://custom", &v);
    v
}

fn dial_for(app: &AppHandle) -> Dial {
    let st = app.state::<AppState>();
    let connected = st.latest.lock().state == ConnectionState::Connected;
    if connected {
        Dial::Socks(crate::engine::exit_socks_port())
    } else {
        Dial::Direct
    }
}

fn added(app: &AppHandle, configs: Vec<CustomConfig>) -> Value {
    let n = configs.len();
    let names: Vec<String> = configs.iter().map(|c| c.name.clone()).collect();
    let s = store::update(|s| s.add(configs));
    crate::log::DiagnosticsLog::i("custom", &format!("Imported {n} custom config(s)."));
    let mut v = publish(app, &s);
    v["added"] = json!(n);
    v["addedNames"] = json!(names);
    v
}

fn pw(password: &Option<String>) -> Option<&str> {
    password.as_deref().filter(|p| !p.is_empty())
}

#[tauri::command]
pub fn cc_state() -> Value {
    view(&store::snapshot())
}

#[tauri::command]
pub async fn cc_import_text(
    app: AppHandle,
    text: String,
    password: Option<String>,
) -> Result<Value, CcError> {
    let dial = dial_for(&app);
    let configs = tauri::async_runtime::spawn_blocking(move || import::from_text(&text, pw(&password), dial))
        .await
        .map_err(|_| CcError { kind: "invalid", message: "import was interrupted".into() })??;
    Ok(added(&app, configs))
}

/// فایلی که خودِ صفحه با `<input type="file">` خوانده.
#[tauri::command]
pub fn cc_import_file(
    app: AppHandle,
    name: String,
    text: String,
    password: Option<String>,
) -> Result<Value, CcError> {
    let configs = import::from_file_text(&name, &text, pw(&password))?;
    Ok(added(&app, configs))
}

/// فایلی که روی پنجره رها شده (مسیرش از رویدادِ drag-drop می‌آید).
#[tauri::command]
pub fn cc_import_path(app: AppHandle, path: String, password: Option<String>) -> Result<Value, CcError> {
    let configs = import::from_file(std::path::Path::new(&path), pw(&password))?;
    Ok(added(&app, configs))
}

#[tauri::command]
pub fn cc_select(app: AppHandle, id: String) -> Value {
    let s = store::update(|s| s.select(&id));
    publish(&app, &s)
}

#[tauri::command]
pub fn cc_set_mode(app: AppHandle, mode: String) -> Value {
    let m = ChainMode::from_stored_name(Some(&mode));
    let s = store::update(|s| s.set_mode(m));
    publish(&app, &s)
}

#[tauri::command]
pub fn cc_set_enabled(app: AppHandle, enabled: bool) -> Value {
    let s = store::update(|s| s.set_enabled(enabled));
    crate::log::DiagnosticsLog::i(
        "custom",
        if s.enabled { "Custom config switched on (applies from the next connection)." } else { "Custom config switched off." },
    );
    publish(&app, &s)
}

#[tauri::command]
pub fn cc_rename(app: AppHandle, id: String, name: String) -> Value {
    let s = store::update(|s| s.rename(&id, &name));
    publish(&app, &s)
}

/// متنِ قابلِ ویرایش/اشتراک؛ برای کانفیگِ قفل‌شده هرگز.
#[tauri::command]
pub fn cc_get_raw(id: String) -> Result<String, CcError> {
    let s = store::snapshot();
    let c = s.configs.iter().find(|c| c.id == id).ok_or_else(not_found)?;
    if c.locked || c.raw.is_empty() {
        return Err(CcError { kind: "locked", message: "this config is locked by its author".into() });
    }
    Ok(c.raw.clone())
}

#[tauri::command]
pub fn cc_edit(app: AppHandle, id: String, raw: String, password: Option<String>) -> Result<Value, CcError> {
    let s = store::snapshot();
    let old = s.configs.iter().find(|c| c.id == id).ok_or_else(not_found)?.clone();
    if old.locked {
        return Err(CcError { kind: "locked", message: "locked configs cannot be edited".into() });
    }
    let next = import::reparse(&old, &raw, pw(&password))?;
    let s = store::update(|s| s.replace(next));
    Ok(publish(&app, &s))
}

#[tauri::command]
pub fn cc_delete(app: AppHandle, id: String) -> Value {
    let s = store::update(|s| s.delete(&id));
    publish(&app, &s)
}

#[tauri::command]
pub fn cc_delete_all(app: AppHandle) -> Value {
    let s = store::update(|s| s.delete_all());
    crate::log::DiagnosticsLog::i("custom", "All custom configs were deleted.");
    publish(&app, &s)
}

// >>> AETHER-APP-PATCH custom-configs-real-delay
/// یک سنجشِ هم‌زمان کافی است: «Test all» دوباره زده شود، دومی منتظر نمی‌ماند.
static PING_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct PingSlot;
impl Drop for PingSlot {
    fn drop(&mut self) {
        PING_BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn ping_targets(ids: Option<&[String]>) -> Vec<realdelay::Target> {
    let s = store::snapshot();
    s.configs
        .iter()
        .filter(|c| ids.map(|ids| ids.iter().any(|i| i == &c.id)).unwrap_or(true))
        .map(|c| realdelay::Target { id: c.id.clone(), name: c.name.clone(), outbound: c.outbound.clone() })
        .collect()
}

async fn ping_run(app: AppHandle, ids: Option<Vec<String>>) -> Result<Value, CcError> {
    if PING_BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(CcError { kind: "busy", message: "a real-delay test is already running".into() });
    }
    let slot = PingSlot;
    let targets = ping_targets(ids.as_deref());
    if targets.is_empty() {
        return Err(not_found());
    }
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| CcError { kind: "invalid", message: e.to_string() })?;
    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| data_dir.clone());
    let Some(binary) = crate::custom::process::locate(&install_dir, &data_dir) else {
        return Err(CcError {
            kind: "invalid",
            message: "the bundled Xray core (engine\\xray\\xray.exe) is missing — reinstall Aether".into(),
        });
    };
    crate::log::DiagnosticsLog::i("custom", &format!("Real delay: testing {} config(s) through a temporary Xray.", targets.len()));
    let results = tauri::async_runtime::spawn_blocking(move || {
        let _slot = slot;
        realdelay::measure(&binary, &data_dir, &targets)
    })
    .await
    .map_err(|_| CcError { kind: "invalid", message: "the test was interrupted".into() })?;
    let v = json!({ "results": results });
    let _ = app.emit("aether://custom-ping", &v);
    Ok(v)
}

/// «پینگِ واقعی» یک کانفیگ: Xrayِ موقت + `GET /generate_204` از دلِ آن.
#[tauri::command]
pub async fn cc_ping(app: AppHandle, id: String) -> Result<Value, CcError> {
    ping_run(app, Some(vec![id])).await
}

/// «Test all»: همهٔ کانفیگ‌ها، دسته‌های ۳۲تایی در یک Xray.
#[tauri::command]
pub async fn cc_ping_all(app: AppHandle) -> Result<Value, CcError> {
    ping_run(app, None).await
}
// <<< AETHER-APP-PATCH custom-configs-real-delay

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::{ConfigFormat, ConfigProtocol, ConfigSource};

    fn cfg(id: &str, protocol: ConfigProtocol, format: ConfigFormat) -> CustomConfig {
        CustomConfig {
            id: id.into(),
            name: format!("n-{id}"),
            protocol,
            format,
            source: ConfigSource::Link,
            outbound: "{\"protocol\":\"vless\"}".into(),
            raw: String::new(),
            server: "h:1".into(),
            locked: false,
            note: String::new(),
            expires_at: 0,
            mobile_only: false,
            added_at: 5,
        }
    }

    fn labels(v: &Value) -> Vec<String> {
        v["modes"].as_array().unwrap().iter().map(|m| m["label"].as_str().unwrap().to_string()).collect()
    }

    // AETHER-APP-FIX chain-labels-name-the-protocol: پس از ورود، فهرستِ زنجیره و
    // کارتِ وضعیت نامِ پروتکل را نشان می‌دهند و نه «Config»ِ عمومی (مثلِ موبایل).
    #[test]
    fn chain_labels_name_the_imported_protocol() {
        let empty = view(&CcState::default());
        assert!(labels(&empty).iter().any(|l| l == "Config \u{2192} Tor"));

        let npvt = CcState::default()
            .add(vec![cfg("a", ConfigProtocol::Vless, ConfigFormat::Npvt)])
            .set_mode(ChainMode::ConfigTorAether);
        let v = view(&npvt);
        let l = labels(&v);
        assert!(l.iter().all(|x| !x.contains("Config")), "{l:?}");
        assert!(l.iter().any(|x| x == "Npvt \u{2192} Aether"), "{l:?}");
        assert_eq!(v["modeLabel"], "Npvt \u{2192} Tor \u{2192} Aether");

        let vmess = CcState::default().add(vec![cfg("b", ConfigProtocol::Vmess, ConfigFormat::Uri)]);
        assert!(labels(&view(&vmess)).iter().any(|x| x == "Psiphon \u{2192} VMess"));
    }
}
