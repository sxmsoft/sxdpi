use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

// ─── Hata Tipleri ───────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum SettingsError {
    #[error("Ayar dosyası okunamadı: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Ayar dosyası parse edilemedi: {0}")]
    ParseError(#[from] serde_json::Error),
}

// ─── DPI Bypass Modları ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BypassMode {
    /// Sadece TCP fragmentation — TLS ClientHello'yu parçalar
    TcpFragmentation,
    /// Fake paket enjeksiyonu — düşük TTL ile sahte SNI gönderir
    FakePacket,
    /// Host header manipülasyonu — HTTP Host header'ını karıştırır
    HostManipulation,
    /// Tüm teknikleri birlikte uygula
    Combined,
}

impl Default for BypassMode {
    fn default() -> Self {
        Self::TcpFragmentation
    }
}

impl std::fmt::Display for BypassMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BypassMode::TcpFragmentation => write!(f, "TCP Fragmentation"),
            BypassMode::FakePacket => write!(f, "Fake Packet"),
            BypassMode::HostManipulation => write!(f, "Host Manipulation"),
            BypassMode::Combined => write!(f, "Combined"),
        }
    }
}

// ─── Ayarlar Yapısı ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Aktif DPI bypass modu
    pub bypass_mode: BypassMode,

    /// Dil ayarı (varsayılan: "en")
    pub language: String,

    /// İlk TCP parçasının boyutu (byte cinsinden, varsayılan: 2)
    /// ClientHello'nun SNI alanını bölmek için küçük tutulur
    pub fragment_size: usize,

    /// TCP parçaları arasındaki gecikme (milisaniye, varsayılan: 50)
    /// DPI cihazının timeout'una düşmesini sağlar
    pub fragment_delay_ms: u64,

    /// Yerel proxy sunucu portu (varsayılan: 8118)
    pub proxy_port: u16,

    /// Sistem başlangıcında otomatik çalıştır
    pub autostart: bool,

    /// Fake paket TTL değeri (varsayılan: 1)
    /// Paket DPI'dan geçer ama hedefe ulaşmadan ölür
    pub ttl_value: u8,

    /// Host header'ı rastgele büyük/küçük harf karışımı yap
    /// Örn: "Host" → "hOsT" (bazı DPI sistemlerini şaşırtır)
    pub enable_host_mixcase: bool,

    /// Host alanının sonuna nokta ekle
    /// Örn: "example.com" → "example.com." (DNS standardına uygun ama DPI'yı şaşırtır)
    pub enable_dot_after_host: bool,

    /// HTTP isteklerinde ek boşluk / satırsonu enjekte et
    /// Örn: "GET / HTTP/1.1\r\n" → "GET / HTTP/1.1\r\n \r\n"
    pub enable_header_padding: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass_mode: BypassMode::default(),
            language: "en".to_string(),
            fragment_size: 2,
            fragment_delay_ms: 50,
            proxy_port: 8118,
            autostart: false,
            ttl_value: 1,
            enable_host_mixcase: true,
            enable_dot_after_host: false,
            enable_header_padding: false,
        }
    }
}

impl Settings {
    /// Ayarları JSON dosyasından yükler.
    /// Dosya yoksa varsayılan ayarları oluşturur ve kaydeder.
    pub fn load() -> Result<Self, SettingsError> {
        let path = Self::config_path();

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let settings: Settings = serde_json::from_str(&content)?;
            log::info!("Ayarlar yüklendi: {}", path.display());
            Ok(settings)
        } else {
            log::info!("Ayar dosyası bulunamadı, varsayılanlar oluşturuluyor.");
            let defaults = Settings::default();
            defaults.save()?;
            Ok(defaults)
        }
    }

    /// Ayarları JSON dosyasına kaydeder.
    /// Dizin yoksa oluşturur.
    pub fn save(&self) -> Result<(), SettingsError> {
        let path = Self::config_path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        log::info!("Ayarlar kaydedildi: {}", path.display());

        Ok(())
    }

    /// Ayar dosyasının tam yolunu döner.
    /// Windows: %APPDATA%/SxDPI/settings.json
    /// Linux:   ~/.config/SxDPI/settings.json
    fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("SxDPI").join("settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = Settings::default();
        assert_eq!(s.fragment_size, 2);
        assert_eq!(s.proxy_port, 8118);
        assert_eq!(s.bypass_mode, BypassMode::TcpFragmentation);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.fragment_size, s.fragment_size);
        assert_eq!(parsed.proxy_port, s.proxy_port);
    }
}
