use std::process::Command;
use thiserror::Error;

// ─── Hata Tipleri ───────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Sistem proxy ayarı değiştirilemedi: {0}")]
    SetFailed(String),

    #[error("Sistem proxy ayarı temizlenemedi: {0}")]
    UnsetFailed(String),

    #[error("Komut çalıştırılamadı: {0}")]
    CommandError(#[from] std::io::Error),
}

// ─── Windows Proxy Yönetimi ─────────────────────────────────────────────────

/// Windows'ta Internet Explorer/System proxy ayarlarını Registry üzerinden değiştirir.
/// Bu ayarlar Chrome, Edge ve diğer sistem proxy'sini kullanan uygulamaları etkiler.
#[cfg(target_os = "windows")]
pub fn set_system_proxy(port: u16) -> Result<(), ProxyError> {
    let proxy_addr = format!("127.0.0.1:{}", port);
    log::info!("Windows sistem proxy ayarlanıyor: {}", proxy_addr);

    // Registry: ProxyEnable = 1 (proxy aktif)
    let enable_result = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v", "ProxyEnable",
            "/t", "REG_DWORD",
            "/d", "1",
            "/f",
        ])
        .output()?;

    if !enable_result.status.success() {
        return Err(ProxyError::SetFailed(
            String::from_utf8_lossy(&enable_result.stderr).to_string(),
        ));
    }

    // Registry: ProxyServer = "127.0.0.1:<port>"
    let server_result = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v", "ProxyServer",
            "/t", "REG_SZ",
            "/d", &proxy_addr,
            "/f",
        ])
        .output()?;

    if !server_result.status.success() {
        return Err(ProxyError::SetFailed(
            String::from_utf8_lossy(&server_result.stderr).to_string(),
        ));
    }

    // Localhost'u bypass listesine ekle (proxy loop'u önler)
    let bypass_result = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v", "ProxyOverride",
            "/t", "REG_SZ",
            "/d", "localhost;127.0.0.1;<local>",
            "/f",
        ])
        .output()?;

    if !bypass_result.status.success() {
        log::warn!(
            "Proxy bypass listesi ayarlanamadı: {}",
            String::from_utf8_lossy(&bypass_result.stderr)
        );
    }

    // InternetSetOption çağrısı yerine — ayarların hemen etkili olması için
    // rundll32 ile Internet Settings'i yeniliyoruz
    let _ = Command::new("rundll32.exe")
        .args(["wininet.dll,InternetSetOptionW"])
        .output();

    log::info!("Windows sistem proxy başarıyla ayarlandı: {}", proxy_addr);
    Ok(())
}

/// Windows'ta sistem proxy ayarlarını devre dışı bırakır.
#[cfg(target_os = "windows")]
pub fn unset_system_proxy() -> Result<(), ProxyError> {
    log::info!("Windows sistem proxy devre dışı bırakılıyor.");

    // Registry: ProxyEnable = 0 (proxy devre dışı)
    let disable_result = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v", "ProxyEnable",
            "/t", "REG_DWORD",
            "/d", "0",
            "/f",
        ])
        .output()?;

    if !disable_result.status.success() {
        return Err(ProxyError::UnsetFailed(
            String::from_utf8_lossy(&disable_result.stderr).to_string(),
        ));
    }

    // Ayarları yenile
    let _ = Command::new("rundll32.exe")
        .args(["wininet.dll,InternetSetOptionW"])
        .output();

    log::info!("Windows sistem proxy başarıyla devre dışı bırakıldı.");
    Ok(())
}

// ─── Linux Proxy Yönetimi ───────────────────────────────────────────────────

/// Linux'ta GNOME desktop ortamı için gsettings üzerinden proxy ayarlar.
/// KDE/diğer ortamlar için ortam değişkenleri kullanılır.
#[cfg(target_os = "linux")]
pub fn set_system_proxy(port: u16) -> Result<(), ProxyError> {
    let proxy_addr = format!("127.0.0.1:{}", port);
    log::info!("Linux sistem proxy ayarlanıyor: {}", proxy_addr);

    // GNOME gsettings ile proxy ayarla
    let gsettings_available = Command::new("which")
        .arg("gsettings")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if gsettings_available {
        // Proxy modunu 'manual' yap
        let _ = Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy", "mode", "'manual'"])
            .output()?;

        // HTTP proxy
        let _ = Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", "'127.0.0.1'"])
            .output()?;
        let _ = Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &port.to_string(),
            ])
            .output()?;

        // HTTPS proxy
        let _ = Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", "'127.0.0.1'"])
            .output()?;
        let _ = Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &port.to_string(),
            ])
            .output()?;

        log::info!("GNOME proxy ayarları güncellendi: {}", proxy_addr);
    } else {
        log::warn!(
            "gsettings bulunamadı. Ortam değişkenleri ile proxy ayarlanamaz (session-scoped). \
             Kullanıcının http_proxy={0} ve https_proxy={0} ortam değişkenlerini \
             manuel ayarlaması gerekebilir.",
            proxy_addr
        );
    }

    Ok(())
}

/// Linux'ta sistem proxy ayarlarını devre dışı bırakır.
#[cfg(target_os = "linux")]
pub fn unset_system_proxy() -> Result<(), ProxyError> {
    log::info!("Linux sistem proxy devre dışı bırakılıyor.");

    let gsettings_available = Command::new("which")
        .arg("gsettings")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if gsettings_available {
        let _ = Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy", "mode", "'none'"])
            .output()?;

        log::info!("GNOME proxy ayarları temizlendi.");
    }

    Ok(())
}

// ─── Diğer Platformlar (Fallback) ──────────────────────────────────────────

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn set_system_proxy(port: u16) -> Result<(), ProxyError> {
    log::warn!(
        "Bu platform için sistem proxy desteği henüz eklenmedi. Port: {}",
        port
    );
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn unset_system_proxy() -> Result<(), ProxyError> {
    log::warn!("Bu platform için sistem proxy temizleme desteği henüz eklenmedi.");
    Ok(())
}

// ─── Autostart Yönetimi ─────────────────────────────────────────────────────

/// Uygulamayı sistem başlangıcına ekler.
#[cfg(target_os = "windows")]
pub fn enable_autostart(exe_path: &str) -> Result<(), ProxyError> {
    log::info!("Autostart etkinleştiriliyor: {}", exe_path);

    let result = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v", "SxDPI",
            "/t", "REG_SZ",
            "/d", exe_path,
            "/f",
        ])
        .output()?;

    if !result.status.success() {
        return Err(ProxyError::SetFailed(
            String::from_utf8_lossy(&result.stderr).to_string(),
        ));
    }

    log::info!("Autostart başarıyla etkinleştirildi.");
    Ok(())
}

/// Uygulamayı sistem başlangıcından kaldırır.
#[cfg(target_os = "windows")]
pub fn disable_autostart() -> Result<(), ProxyError> {
    log::info!("Autostart devre dışı bırakılıyor.");

    let result = Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v", "SxDPI",
            "/f",
        ])
        .output()?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        // Kayıt zaten yoksa hata verme
        if !stderr.contains("unable to find") {
            return Err(ProxyError::UnsetFailed(stderr));
        }
    }

    log::info!("Autostart başarıyla devre dışı bırakıldı.");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn enable_autostart(_exe_path: &str) -> Result<(), ProxyError> {
    log::info!("Linux autostart etkinleştiriliyor.");

    let autostart_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("autostart");

    std::fs::create_dir_all(&autostart_dir)?;

    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=SxDPI\n\
         Comment=DPI Bypass Application\n\
         Exec={}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n",
        _exe_path
    );

    let desktop_path = autostart_dir.join("sxdpi.desktop");
    std::fs::write(&desktop_path, desktop_entry)?;

    log::info!("Linux autostart dosyası oluşturuldu: {}", desktop_path.display());
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn disable_autostart() -> Result<(), ProxyError> {
    log::info!("Linux autostart devre dışı bırakılıyor.");

    let desktop_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.config"))
        .join("autostart")
        .join("sxdpi.desktop");

    if desktop_path.exists() {
        std::fs::remove_file(&desktop_path)?;
        log::info!("Autostart dosyası silindi.");
    }

    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn enable_autostart(_exe_path: &str) -> Result<(), ProxyError> {
    log::warn!("Bu platform için autostart desteği henüz eklenmedi.");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn disable_autostart() -> Result<(), ProxyError> {
    log::warn!("Bu platform için autostart desteği henüz eklenmedi.");
    Ok(())
}
