use crate::dns;
use crate::settings::{BypassMode, Settings};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};

// ─── Hata Tipleri ───────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Motor zaten çalışıyor")]
    AlreadyRunning,

    #[error("Motor zaten durmuş")]
    AlreadyStopped,

    #[error("Proxy sunucu başlatılamadı: {0}")]
    BindError(String),

    #[error("IO hatası: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Bağlantı hatası: {0}")]
    ConnectionError(String),
}

// ─── Motor Durumları ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

impl std::fmt::Display for EngineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineState::Stopped => write!(f, "Stopped"),
            EngineState::Starting => write!(f, "Starting"),
            EngineState::Running => write!(f, "Running"),
            EngineState::Stopping => write!(f, "Stopping"),
        }
    }
}

// ─── DPI Engine ─────────────────────────────────────────────────────────────

pub struct DpiEngine {
    state: EngineState,
    settings: Settings,
    shutdown_signal: Arc<Notify>,
}

impl DpiEngine {
    pub fn new(settings: Settings) -> Self {
        Self {
            state: EngineState::Stopped,
            settings,
            shutdown_signal: Arc::new(Notify::new()),
        }
    }

    pub fn state(&self) -> &EngineState {
        &self.state
    }

    pub fn update_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub async fn start(&mut self) -> Result<(), EngineError> {
        if self.state == EngineState::Running || self.state == EngineState::Starting {
            return Err(EngineError::AlreadyRunning);
        }

        self.state = EngineState::Starting;
        let port = self.settings.proxy_port;
        let bind_addr = format!("127.0.0.1:{}", port);

        log::info!("DPI Engine başlatılıyor: {}", bind_addr);

        let listener = TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| EngineError::BindError(format!("{}: {}", bind_addr, e)))?;

        log::info!("Proxy dinleniyor: {}", bind_addr);

        let shutdown = self.shutdown_signal.clone();
        let settings = self.settings.clone();

        tokio::spawn(async move {
            Self::proxy_loop(listener, shutdown, settings).await;
        });

        self.state = EngineState::Running;
        log::info!("DPI Engine çalışıyor. Mod: {}", self.settings.bypass_mode);

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), EngineError> {
        if self.state == EngineState::Stopped || self.state == EngineState::Stopping {
            return Err(EngineError::AlreadyStopped);
        }

        self.state = EngineState::Stopping;
        log::info!("DPI Engine durduruluyor...");

        self.shutdown_signal.notify_waiters();
        self.shutdown_signal = Arc::new(Notify::new());

        self.state = EngineState::Stopped;
        log::info!("DPI Engine durduruldu.");

        Ok(())
    }

    // ─── Proxy Döngüsü ─────────────────────────────────────────────────────

    async fn proxy_loop(listener: TcpListener, shutdown: Arc<Notify>, settings: Settings) {
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((client_stream, peer_addr)) => {
                            log::info!("Bağlantı: {}", peer_addr);
                            let settings = settings.clone();
                            let shutdown = shutdown.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    client_stream, settings, shutdown
                                ).await {
                                    log::warn!("Bağlantı hatası ({}): {}", peer_addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            log::error!("Kabul hatası: {}", e);
                            sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = shutdown.notified() => {
                    log::info!("Proxy döngüsü sonlandırılıyor.");
                    break;
                }
            }
        }
    }

    // ─── Bağlantı İşleme ───────────────────────────────────────────────────

    async fn handle_connection(
        mut client: TcpStream,
        settings: Settings,
        shutdown: Arc<Notify>,
    ) -> Result<(), EngineError> {
        client.set_nodelay(true)?;

        let mut initial_buf = vec![0u8; 16384];
        let n = client.read(&mut initial_buf).await?;
        if n == 0 {
            return Ok(());
        }
        let initial_data = &initial_buf[..n];
        let request_str = String::from_utf8_lossy(initial_data);

        if request_str.starts_with("CONNECT ") {
            Self::handle_connect(client, initial_data, &settings, shutdown).await
        } else if Self::is_http_request(&request_str) {
            Self::handle_http(client, initial_data, &settings, shutdown).await
        } else {
            log::warn!("Bilinmeyen protokol ({} byte), kapatılıyor", n);
            Ok(())
        }
    }

    fn is_http_request(data: &str) -> bool {
        let methods = ["GET ", "POST ", "PUT ", "DELETE ", "HEAD ", "OPTIONS ", "PATCH "];
        methods.iter().any(|m| data.starts_with(m))
    }

    // ─── HTTPS CONNECT Handler ──────────────────────────────────────────────

    async fn handle_connect(
        mut client: TcpStream,
        initial_data: &[u8],
        settings: &Settings,
        shutdown: Arc<Notify>,
    ) -> Result<(), EngineError> {
        let request_str = String::from_utf8_lossy(initial_data);

        let target_addr = request_str
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| EngineError::ConnectionError("Geçersiz CONNECT".into()))?
            .to_string();

        let target_addr = if target_addr.contains(':') {
            target_addr
        } else {
            format!("{}:443", target_addr)
        };

        log::info!("CONNECT → {}", target_addr);

        // Güvenli DNS ile çözümle ve bağlan
        let mut server = match Self::connect_resolved(&target_addr).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Bağlantı başarısız: {} — {}", target_addr, e);
                let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Err(e);
            }
        };
        server.set_nodelay(true)?;

        // Client'a 200 yanıtı gönder
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;

        // TLS ClientHello'yu yakala
        let mut tls_buf = vec![0u8; 16384];
        let tls_n = tokio::select! {
            result = client.read(&mut tls_buf) => result?,
            _ = sleep(Duration::from_secs(10)) => {
                log::warn!("TLS timeout: {}", target_addr);
                return Err(EngineError::ConnectionError("TLS timeout".into()));
            }
        };

        if tls_n == 0 {
            return Ok(());
        }

        let tls_data = &tls_buf[..tls_n];

        // TLS ClientHello kontrolü: ContentType=0x16, HandshakeType=0x01
        let is_client_hello = tls_n >= 6
            && tls_data[0] == 0x16
            && tls_data[5] == 0x01;

        if is_client_hello {
            log::info!("TLS ClientHello ({} byte) → bypass: {}", tls_n, target_addr);
            Self::send_with_bypass(&mut server, tls_data, settings).await?;
        } else {
            log::info!("TLS olmayan veri ({} byte) → direkt: {}", tls_n, target_addr);
            server.write_all(tls_data).await?;
        }

        // Bidirectional relay
        Self::bidirectional_relay(client, server, shutdown).await
    }

    // ─── HTTP Proxy Handler ─────────────────────────────────────────────────

    async fn handle_http(
        client: TcpStream,
        initial_data: &[u8],
        settings: &Settings,
        shutdown: Arc<Notify>,
    ) -> Result<(), EngineError> {
        let request_str = String::from_utf8_lossy(initial_data);

        let first_line = request_str.lines().next().unwrap_or("");
        let parts: Vec<&str> = first_line.splitn(3, ' ').collect();
        if parts.len() < 3 {
            return Err(EngineError::ConnectionError("Geçersiz HTTP isteği".into()));
        }

        let method = parts[0];
        let url = parts[1];
        let version = parts[2];

        let (host, port, path) = Self::parse_proxy_url(url)?;
        let target_addr = format!("{}:{}", host, port);

        log::info!("HTTP {} {} → {}", method, path, target_addr);

        let mut server = match Self::connect_resolved(&target_addr).await {
            Ok(s) => s,
            Err(e) => {
                let mut client = client;
                let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                return Err(e);
            }
        };
        server.set_nodelay(true)?;

        // İsteği yeniden oluştur: absolute URL → relative URL
        let mut modified_request = format!("{} {} {}\r\n", method, path, version);

        let mut has_host = false;
        for line in request_str.lines().skip(1) {
            if line.is_empty() {
                break;
            }

            let lower = line.to_lowercase();

            if lower.starts_with("host:") {
                has_host = true;
                let host_value = line.splitn(2, ':').nth(1).unwrap_or("").trim();

                let header_name = if settings.enable_host_mixcase {
                    Self::mixcase_string("Host")
                } else {
                    "Host".to_string()
                };

                let host_modified = if settings.enable_dot_after_host {
                    format!("{}.", host_value.trim_end_matches('.'))
                } else {
                    host_value.to_string()
                };

                modified_request.push_str(&format!("{}: {}\r\n", header_name, host_modified));

                if settings.enable_header_padding {
                    modified_request.push_str(&format!("X-Padding: {}\r\n", "x".repeat(32)));
                }
            } else if lower.starts_with("proxy-connection:") {
                let value = line.splitn(2, ':').nth(1).unwrap_or("keep-alive").trim();
                modified_request.push_str(&format!("Connection: {}\r\n", value));
            } else {
                modified_request.push_str(line);
                modified_request.push_str("\r\n");
            }
        }

        if !has_host {
            modified_request.push_str(&format!("Host: {}\r\n", host));
        }

        modified_request.push_str("\r\n");

        // Body varsa ekle
        let mut request_bytes = modified_request.into_bytes();
        if let Some(pos) = find_header_end(initial_data) {
            if pos < initial_data.len() {
                request_bytes.extend_from_slice(&initial_data[pos..]);
            }
        }

        // Gönder
        Self::send_with_bypass(&mut server, &request_bytes, settings).await?;

        // Bidirectional relay
        Self::bidirectional_relay(client, server, shutdown).await
    }

    /// "http://example.com:8080/path" → ("example.com", 8080, "/path")
    fn parse_proxy_url(url: &str) -> Result<(String, u16, String), EngineError> {
        if url.starts_with('/') {
            return Err(EngineError::ConnectionError("Relative URL".into()));
        }

        let without_scheme = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .unwrap_or(url);

        let (host_port, path) = match without_scheme.find('/') {
            Some(idx) => (&without_scheme[..idx], without_scheme[idx..].to_string()),
            None => (without_scheme, "/".to_string()),
        };

        let (host, port) = if let Some(colon_idx) = host_port.rfind(':') {
            let port_str = &host_port[colon_idx + 1..];
            if let Ok(port) = port_str.parse::<u16>() {
                (host_port[..colon_idx].to_string(), port)
            } else {
                (host_port.to_string(), 80)
            }
        } else {
            (host_port.to_string(), 80)
        };

        if host.is_empty() {
            return Err(EngineError::ConnectionError("Boş host".into()));
        }

        Ok((host, port, path))
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  DPI BYPASS TEKNİKLERİ — GoodbyeDPI'dan esinlenerek
    //
    //  GoodbyeDPI'ın temel yaklaşımı:
    //  • İlk birkaç byte'ı ayrı TCP segmenti olarak gönder (SNI/Host'u böl)
    //  • Kalanını tek seferde gönder (hız kaybetme!)
    //  • Tüm paketi parçalama → timeout'a yol açar
    // ═══════════════════════════════════════════════════════════════════════

    async fn send_with_bypass(
        server: &mut TcpStream,
        data: &[u8],
        settings: &Settings,
    ) -> Result<(), EngineError> {
        match settings.bypass_mode {
            BypassMode::TcpFragmentation => {
                Self::send_fragmented(server, data, settings).await
            }
            BypassMode::FakePacket => {
                Self::send_with_desync(server, data, settings).await
            }
            BypassMode::HostManipulation => {
                // Host manipülasyonu zaten handle_http'de uygulandı.
                // Burada sadece basit 2-parça fragmentation uygula.
                Self::send_fragmented(server, data, settings).await
            }
            BypassMode::Combined => {
                Self::send_combined(server, data, settings).await
            }
        }
    }

    /// TCP Fragmentation — GoodbyeDPI tarzı.
    ///
    /// GoodbyeDPI sadece İLK birkaç byte'ı ayırır, sonra kalanını tek seferde gönderir.
    /// Bu, SNI/Host bilgisini ilk TCP segmentinden bölerek DPI'ın onu
    /// tespit etmesini engeller.
    ///
    /// ÖNEMLİ: Tüm paketi küçük parçalara bölmek yanlıştır! Timeout'a yol açar.
    async fn send_fragmented(
        server: &mut TcpStream,
        data: &[u8],
        settings: &Settings,
    ) -> Result<(), EngineError> {
        let frag_size = settings.fragment_size.max(1).min(data.len());
        let delay_ms = settings.fragment_delay_ms;

        log::info!(
            "Fragment: {} byte veri → ilk {} byte ayrı, kalan {} byte toplu",
            data.len(), frag_size, data.len().saturating_sub(frag_size)
        );

        // 1. İlk fragment_size byte'ı gönder (SNI/Host'u kırmak için)
        server.write_all(&data[..frag_size]).await?;
        server.flush().await?;

        // 2. Kısa gecikme — DPI'ın ilk segmenti işlemesini bekle
        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms)).await;
        }

        // 3. Kalanını TEK SEFERDE gönder
        if frag_size < data.len() {
            server.write_all(&data[frag_size..]).await?;
            server.flush().await?;
        }

        Ok(())
    }

    /// Desync tekniği — GoodbyeDPI'ın "first byte + delay" yaklaşımı.
    ///
    /// 1. İlk 1 byte gönder (DPI stream takibini başlat)
    /// 2. Gecikme (DPI reassembly buffer'ını zorla)
    /// 3. fragment_size kadar gönder (SNI'ı kes)
    /// 4. Kalanını tek seferde gönder
    async fn send_with_desync(
        server: &mut TcpStream,
        data: &[u8],
        settings: &Settings,
    ) -> Result<(), EngineError> {
        if data.len() <= 3 {
            server.write_all(data).await?;
            return Ok(());
        }

        let frag_size = settings.fragment_size.max(2).min(data.len() - 1);
        let delay = Duration::from_millis(settings.fragment_delay_ms);

        log::info!("Desync: {} byte veri, frag={}", data.len(), frag_size);

        // 1. İlk 1 byte (desync trigger)
        server.write_all(&data[..1]).await?;
        server.flush().await?;
        sleep(delay).await;

        // 2. fragment_size kadar gönder
        let end = (1 + frag_size).min(data.len());
        server.write_all(&data[1..end]).await?;
        server.flush().await?;
        sleep(delay).await;

        // 3. Kalanını tek seferde
        if end < data.len() {
            server.write_all(&data[end..]).await?;
            server.flush().await?;
        }

        Ok(())
    }

    /// Combined — GoodbyeDPI mode -9 benzeri.
    ///
    /// 3 parçaya böler: [ilk 1 byte] + [sonraki fragment_size byte] + [kalan tümü]
    /// Her parça arasında kısa gecikme.
    async fn send_combined(
        server: &mut TcpStream,
        data: &[u8],
        settings: &Settings,
    ) -> Result<(), EngineError> {
        if data.len() <= 2 {
            server.write_all(data).await?;
            return Ok(());
        }

        let frag_size = settings.fragment_size.max(2).min(data.len() - 1);
        let delay = Duration::from_millis(settings.fragment_delay_ms);

        log::info!("Combined: {} byte → 1 + {} + {} byte", 
            data.len(), 
            frag_size.min(data.len() - 1),
            data.len().saturating_sub(1 + frag_size)
        );

        // Aşama 1: İlk byte (desync)
        server.write_all(&data[..1]).await?;
        server.flush().await?;
        sleep(delay).await;

        // Aşama 2: Sonraki fragment_size byte (SNI kırma)
        let mid = (1 + frag_size).min(data.len());
        server.write_all(&data[1..mid]).await?;
        server.flush().await?;
        sleep(delay).await;

        // Aşama 3: Kalanını tek seferde
        if mid < data.len() {
            server.write_all(&data[mid..]).await?;
            server.flush().await?;
        }

        Ok(())
    }

    // ─── Yardımcılar ────────────────────────────────────────────────────────

    /// Güvenli DNS üzerinden hostname'i çözümleyip TCP bağlantısı kurar.
    /// ISP DNS poisoning'ini bypass eder.
    async fn connect_resolved(target: &str) -> Result<TcpStream, EngineError> {
        // host:port olarak ayır
        let (host, port_str) = target.rsplit_once(':')
            .ok_or_else(|| EngineError::ConnectionError(format!("Geçersiz adres: {}", target)))?;
        let port: u16 = port_str.parse()
            .map_err(|_| EngineError::ConnectionError(format!("Geçersiz port: {}", port_str)))?;

        // Güvenli DNS ile çözümle
        let ip = dns::resolve(host).await
            .map_err(|e| EngineError::ConnectionError(format!("{}: {}", target, e)))?;

        let addr = SocketAddr::new(ip, port);
        log::info!("DNS: {} → {} (bağlanılıyor...)", target, addr);

        let stream = TcpStream::connect(addr).await
            .map_err(|e| EngineError::ConnectionError(format!("{} ({}): {}", target, addr, e)))?;

        Ok(stream)
    }

    fn mixcase_string(s: &str) -> String {
        s.chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_lowercase().to_string()
                } else {
                    c.to_uppercase().to_string()
                }
            })
            .collect()
    }

    // ─── Bidirectional Relay ────────────────────────────────────────────────

    async fn bidirectional_relay(
        client: TcpStream,
        server: TcpStream,
        shutdown: Arc<Notify>,
    ) -> Result<(), EngineError> {
        let (mut client_reader, mut client_writer) = client.into_split();
        let (mut server_reader, mut server_writer) = server.into_split();

        let shutdown_c2s = shutdown.clone();

        // Client → Server
        let c2s = tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                tokio::select! {
                    result = client_reader.read(&mut buf) => {
                        match result {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if server_writer.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    _ = shutdown_c2s.notified() => break,
                }
            }
            let _ = server_writer.shutdown().await;
        });

        // Server → Client
        let s2c = tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                tokio::select! {
                    result = server_reader.read(&mut buf) => {
                        match result {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if client_writer.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    _ = shutdown.notified() => break,
                }
            }
            let _ = client_writer.shutdown().await;
        });

        let _ = tokio::join!(c2s, s2c);
        Ok(())
    }
}

// ─── Yardımcı Fonksiyonlar ──────────────────────────────────────────────────

/// HTTP header sonunu (\r\n\r\n) bulur ve sonraki byte'ın index'ini döner.
fn find_header_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == b'\r'
            && data[i + 1] == b'\n'
            && data[i + 2] == b'\r'
            && data[i + 3] == b'\n'
        {
            return Some(i + 4);
        }
    }
    None
}
