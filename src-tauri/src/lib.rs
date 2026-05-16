mod commands;
mod dns;
mod dpi_engine;
mod settings;
mod system_proxy;

use commands::EngineState;
use dpi_engine::DpiEngine;
use settings::Settings;
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent, RunEvent,
};
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Logger başlat
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    // Çökme durumunda proxy'yi temizle (Panic Handler)
    std::panic::set_hook(Box::new(|info| {
        log::error!("Kritik hata (Panic): {:?}", info);
        let _ = crate::system_proxy::unset_system_proxy();
    }));

    log::info!("SxDPI başlatılıyor...");

    // Ayarları yükle
    let settings = Settings::load().unwrap_or_default();

    // DPI Engine'i oluştur (thread-safe)
    let engine = Arc::new(Mutex::new(DpiEngine::new(settings)));
    let engine_state = EngineState(engine);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // ─── State Yönetimi ─────────────────────────────────────────
        .manage(engine_state)
        // ─── Tauri Command'leri ─────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            commands::connect_dpi,
            commands::disconnect_dpi,
            commands::get_status,
            commands::save_settings,
            commands::load_settings,
            commands::flush_dpi,
        ])
        // ─── Sistem Tray + Window Setup ─────────────────────────────
        .setup(|app| {
            // ── Tray Menü Öğeleri ───────────────────────────────────
            let show_hide_item =
                MenuItem::with_id(app, "show_hide", "Göster/Gizle", true, None::<&str>)?;
            let connect_item =
                MenuItem::with_id(app, "connect", "Bağlan", true, None::<&str>)?;
            let disconnect_item =
                MenuItem::with_id(app, "disconnect", "Bağlantıyı Kes", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;

            // ── Tray Menüsü ────────────────────────────────────────
            let tray_menu = MenuBuilder::new(app)
                .item(&show_hide_item)
                .separator()
                .item(&connect_item)
                .item(&disconnect_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // ── Tray İkonu ─────────────────────────────────────────
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .tooltip("SxDPI — DPI Bypass")
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        // ── Göster/Gizle ────────────────────────────
                        "show_hide" => {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        // ── Bağlan ──────────────────────────────────
                        "connect" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_handle.state::<EngineState>();
                                match commands::connect_dpi(state).await {
                                    Ok(msg) => log::info!("Tray: {}", msg),
                                    Err(e) => log::error!("Tray bağlantı hatası: {}", e),
                                }
                                // Frontend'e durum güncellemesi gönder
                                let _ = app_handle.emit("engine-state-changed", "running");
                            });
                        }
                        // ── Bağlantıyı Kes ─────────────────────────
                        "disconnect" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_handle.state::<EngineState>();
                                match commands::disconnect_dpi(state).await {
                                    Ok(msg) => log::info!("Tray: {}", msg),
                                    Err(e) => log::error!("Tray bağlantı kesme hatası: {}", e),
                                }
                                let _ = app_handle.emit("engine-state-changed", "stopped");
                            });
                        }
                        // ── Çıkış ───────────────────────────────────
                        "quit" => {
                            log::info!("Çıkış yapılıyor...");
                            // Motoru durdur ve proxy'yi temizle
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_handle.state::<EngineState>();
                                let _ = commands::disconnect_dpi(state).await;
                                app_handle.exit(0);
                            });
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            log::info!("Sistem tray başarıyla oluşturuldu.");
            Ok(())
        })
        // ─── Window Olayları ────────────────────────────────────────
        .on_window_event(|window, event| {
            // Pencere kapatılırken tray'e küçült (tamamen kapatma)
            if let WindowEvent::CloseRequested { api, .. } = event {
                log::info!("Pencere kapatılıyor → tray'e küçültülüyor.");
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("SxDPI uygulaması başlatılırken hata oluştu")
        .run(|_app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                log::info!("Uygulama kapanıyor, proxy temizleniyor...");
                let _ = crate::system_proxy::unset_system_proxy();
            } else if let RunEvent::Exit = event {
                log::info!("Uygulama kapandı, proxy temizleniyor...");
                let _ = crate::system_proxy::unset_system_proxy();
            }
        });
}
