use std::ffi::c_void;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
extern "system" {
    fn RegDeleteKeyValueW(hkey: isize, lpsubkey: *const u16, lpvaluename: *const u16) -> i32;
    fn RegSetKeyValueW(
        hkey: isize,
        lpsubkey: *const u16,
        lpvaluename: *const u16,
        dwtype: u32,
        lpdata: *const c_void,
        cbdata: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        hinternet: *const c_void,
        dwoption: u32,
        lpbuffer: *const c_void,
        dwbufferlength: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SendMessageTimeoutW(
        hwnd: isize,
        msg: u32,
        wparam: usize,
        lparam: isize,
        flags: u32,
        timeout: u32,
        result: *mut usize,
    ) -> isize;
}

#[cfg(target_os = "windows")]
const INTERNET_OPTION_REFRESH: u32 = 37;
#[cfg(target_os = "windows")]
const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
#[cfg(target_os = "windows")]
const HWND_BROADCAST: isize = 0xffff;
#[cfg(target_os = "windows")]
const WM_SETTINGCHANGE: u32 = 0x001a;
#[cfg(target_os = "windows")]
const SMTO_ABORTIFHUNG: u32 = 0x0002;
#[cfg(target_os = "windows")]
const HKEY_CURRENT_USER: isize = 0x80000001u32 as isize;
#[cfg(target_os = "windows")]
const ERROR_FILE_NOT_FOUND: i32 = 2;
#[cfg(target_os = "windows")]
const REG_SZ: u32 = 1;
#[cfg(target_os = "windows")]
const REG_DWORD: u32 = 4;

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
const WINDOWS_PROXY_OVERRIDE: &str = concat!(
    "localhost;127.0.0.1;127.*;<local>;",
    "10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;",
    "172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;",
    "*.microsoft.com;*.microsoftonline.com;*.windows.com;*.windows.net;*.windowsupdate.com;",
    "*.mp.microsoft.com;*.delivery.mp.microsoft.com;*.s-microsoft.com;*.msedge.net;*.azureedge.net;",
    "*.trafficmanager.net;*.akamaized.net;*.akamaihd.net;*.akadns.net;*.live.com;*.xboxlive.com;*.xboxservices.com;*.bing.com"
);

pub fn begin_shutdown_cleanup() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
}

fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

#[cfg(target_os = "windows")]
fn notify_proxy_settings_changed() {
    if is_shutting_down() {
        return;
    }

    unsafe {
        let changed = InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null(),
            0,
        );
        let refreshed = InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null(),
            0,
        );

        if changed == 0 || refreshed == 0 {
            log::warn!("WinINet proxy change notification did not fully succeed");
        }

        let setting = widestring("Internet Settings");
        let mut result = 0usize;
        let broadcast = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            setting.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            250,
            &mut result,
        );
        if broadcast == 0 {
            log::warn!("WM_SETTINGCHANGE broadcast for proxy settings did not fully succeed");
        }
    }
}

#[cfg(target_os = "windows")]
fn widestring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn set_reg_value(
    key: &str,
    value: &str,
    reg_type: &str,
    data: &str,
    context: &str,
) -> Result<(), ProxyError> {
    let key_w = widestring(key);
    let value_w = widestring(value);

    let (kind, bytes) = match reg_type {
        "REG_DWORD" => {
            let value = data
                .parse::<u32>()
                .map_err(|e| ProxyError::SetFailed(format!("{}: {}", context, e)))?;
            (REG_DWORD, value.to_le_bytes().to_vec())
        }
        "REG_SZ" => {
            let encoded = widestring(data);
            let bytes = encoded
                .iter()
                .flat_map(|unit| unit.to_le_bytes())
                .collect::<Vec<u8>>();
            (REG_SZ, bytes)
        }
        other => {
            return Err(ProxyError::SetFailed(format!(
                "{}: unsupported registry type {}",
                context, other
            )))
        }
    };

    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key_w.as_ptr(),
            value_w.as_ptr(),
            kind,
            bytes.as_ptr() as *const c_void,
            bytes.len() as u32,
        )
    };

    if status != 0 {
        return Err(ProxyError::SetFailed(format!(
            "{}: Win32 registry status {}",
            context, status
        )));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn delete_reg_value(key: &str, value: &str) {
    let key_w = widestring(key);
    let value_w = widestring(value);
    let status =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key_w.as_ptr(), value_w.as_ptr()) };

    if status != 0 && status != ERROR_FILE_NOT_FOUND {
        log::debug!(
            "Registry value could not be deleted ({}\\{}): Win32 status {}",
            key,
            value,
            status
        );
    }
}

#[cfg(target_os = "windows")]
fn set_winhttp_proxy(proxy_server: &str) {
    if is_shutting_down() {
        return;
    }

    match Command::new("netsh")
        .args([
            "winhttp",
            "set",
            "proxy",
            &format!("proxy-server={}", proxy_server),
            &format!("bypass-list={}", WINDOWS_PROXY_OVERRIDE),
        ])
        .output()
    {
        Ok(result) if !result.status.success() => {
            log::warn!(
                "WinHTTP proxy could not be set. Some installers may ignore SxDPI: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        Err(e) => log::warn!("WinHTTP proxy command failed: {}", e),
        _ => log::info!("WinHTTP proxy updated for apps/installers."),
    }
}

#[cfg(target_os = "windows")]
fn reset_winhttp_proxy() {
    if is_shutting_down() {
        return;
    }

    match Command::new("netsh")
        .args(["winhttp", "reset", "proxy"])
        .output()
    {
        Ok(result) if !result.status.success() => {
            log::warn!(
                "WinHTTP proxy could not be reset: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        Err(e) => log::warn!("WinHTTP proxy reset command failed: {}", e),
        _ => log::info!("WinHTTP proxy reset."),
    }
}

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
    let proxy_server = format!("http={0};https={0}", proxy_addr);
    let internet_settings = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    log::info!("Windows sistem proxy ayarlanıyor: {}", proxy_addr);

    // Registry: ProxyEnable = 1 (proxy aktif)
    set_reg_value(
        internet_settings,
        "ProxyEnable",
        "REG_DWORD",
        "1",
        "ProxyEnable",
    )?;

    // Registry: ProxyServer = "127.0.0.1:<port>"
    set_reg_value(
        internet_settings,
        "ProxyServer",
        "REG_SZ",
        &proxy_server,
        "ProxyServer",
    )?;

    // Localhost'u bypass listesine ekle (proxy loop'u önler)
    set_reg_value(
        internet_settings,
        "ProxyOverride",
        "REG_SZ",
        WINDOWS_PROXY_OVERRIDE,
        "ProxyOverride",
    )?;

    set_reg_value(
        internet_settings,
        "AutoDetect",
        "REG_DWORD",
        "0",
        "AutoDetect",
    )?;
    delete_reg_value(internet_settings, "AutoConfigURL");
    set_winhttp_proxy(&proxy_server);

    // Notify WinINet consumers so browsers and apps pick this up immediately.
    notify_proxy_settings_changed();

    log::info!("Windows sistem proxy başarıyla ayarlandı: {}", proxy_addr);
    Ok(())
}

/// Windows'ta sistem proxy ayarlarını devre dışı bırakır.
#[cfg(target_os = "windows")]
pub fn unset_system_proxy() -> Result<(), ProxyError> {
    let internet_settings = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    log::info!("Windows sistem proxy devre dışı bırakılıyor.");

    // Registry: ProxyEnable = 0 (proxy devre dışı)
    set_reg_value(
        internet_settings,
        "ProxyEnable",
        "REG_DWORD",
        "0",
        "ProxyEnable",
    )
    .map_err(|e| ProxyError::UnsetFailed(e.to_string()))?;
    reset_winhttp_proxy();

    // Ayarları yenile
    notify_proxy_settings_changed();

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
            "/v",
            "SxDPI",
            "/t",
            "REG_SZ",
            "/d",
            exe_path,
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
            "/v",
            "SxDPI",
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

    log::info!(
        "Linux autostart dosyası oluşturuldu: {}",
        desktop_path.display()
    );
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
