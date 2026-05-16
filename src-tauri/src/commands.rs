use crate::dpi_engine::DpiEngine;
use crate::settings::Settings;
use crate::system_proxy;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

// ─── Shared Engine State ────────────────────────────────────────────────────

/// Thread-safe DPI Engine durumu.
/// Tauri State olarak paylaşılır, tüm command'ler bu state'e erişir.
pub struct EngineState(pub Arc<Mutex<DpiEngine>>);

// ─── Tauri Commands ─────────────────────────────────────────────────────────

/// DPI bypass motorunu başlatır ve sistem proxy'sini ayarlar.
/// Frontend'den: `invoke('connect_dpi')`
#[tauri::command]
pub async fn connect_dpi(engine: State<'_, EngineState>) -> Result<String, String> {
    let mut engine_guard = engine.0.lock().await;

    // Mevcut ayarları al
    let settings = Settings::load().unwrap_or_default();
    let port = settings.proxy_port;

    // Ayarları güncelle
    engine_guard.update_settings(settings);

    // Motoru başlat
    engine_guard
        .start()
        .await
        .map_err(|e| format!("Motor başlatılamadı: {}", e))?;

    // Sistem proxy'sini ayarla
    if let Err(e) = system_proxy::set_system_proxy(port) {
        log::error!("Sistem proxy ayarlanamadı: {}", e);
        // Proxy ayarlanamazsa motoru durdur
        let _ = engine_guard.stop().await;
        return Err(format!("Sistem proxy ayarlanamadı: {}", e));
    }

    log::info!("DPI bypass aktif — port: {}", port);
    Ok(format!("Bağlantı kuruldu (port: {})", port))
}

/// DPI bypass motorunu durdurur ve sistem proxy'sini temizler.
/// Frontend'den: `invoke('disconnect_dpi')`
#[tauri::command]
pub async fn disconnect_dpi(engine: State<'_, EngineState>) -> Result<String, String> {
    let mut engine_guard = engine.0.lock().await;

    // Motoru durdur
    engine_guard
        .stop()
        .await
        .map_err(|e| format!("Motor durdurulamadı: {}", e))?;

    // Sistem proxy'sini temizle
    if let Err(e) = system_proxy::unset_system_proxy() {
        log::error!("Sistem proxy temizlenemedi: {}", e);
        return Err(format!("Sistem proxy temizlenemedi: {}", e));
    }

    log::info!("DPI bypass devre dışı.");
    Ok("Bağlantı kesildi".to_string())
}

/// Motorun mevcut durumunu döner.
/// Frontend'den: `invoke('get_status')`
#[tauri::command]
pub async fn get_status(engine: State<'_, EngineState>) -> Result<String, String> {
    let engine_guard = engine.0.lock().await;
    let state = engine_guard.state().clone();
    Ok(serde_json::to_string(&state).unwrap_or_else(|_| "\"unknown\"".to_string()))
}

/// Ayarları kaydeder.
/// Frontend'den: `invoke('save_settings', { settings: {...} })`
#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<String, String> {
    settings
        .save()
        .map_err(|e| format!("Ayarlar kaydedilemedi: {}", e))?;

    // Autostart ayarını uygula
    if settings.autostart {
        if let Ok(exe) = std::env::current_exe() {
            let exe_str = exe.to_string_lossy().to_string();
            if let Err(e) = system_proxy::enable_autostart(&exe_str) {
                log::warn!("Autostart etkinleştirilemedi: {}", e);
            }
        }
    } else {
        if let Err(e) = system_proxy::disable_autostart() {
            log::warn!("Autostart devre dışı bırakılamadı: {}", e);
        }
    }

    log::info!("Ayarlar kaydedildi.");
    Ok("Ayarlar kaydedildi".to_string())
}

/// Kayıtlı ayarları yükler.
/// Frontend'den: `invoke('load_settings')`
#[tauri::command]
pub async fn load_settings() -> Result<Settings, String> {
    Settings::load().map_err(|e| format!("Ayarlar yüklenemedi: {}", e))
}
