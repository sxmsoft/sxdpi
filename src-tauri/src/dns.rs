use std::net::{IpAddr, Ipv4Addr};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct DoHResponse {
    #[serde(rename = "Status")]
    status: u32,
    #[serde(rename = "Answer")]
    answer: Option<Vec<DoHAnswer>>,
}

#[derive(Deserialize)]
struct DoHAnswer {
    #[serde(rename = "type")]
    record_type: u16,
    data: String,
}

/// Hostname'i DNS-over-HTTPS (DoH) ile güvenilir sunucular üzerinden çözümler.
/// Bu yöntem port 53'ü kullanmadığı için ISP'nin DNS engellemelerini bypass eder.
pub async fn resolve(hostname: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = hostname.parse::<IpAddr>() {
        return Ok(ip);
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .map_err(|e| format!("Client hatası: {}", e))?;

    // Cloudflare ve Google DoH JSON API'leri
    let endpoints = [
        "https://cloudflare-dns.com/dns-query",
        "https://dns.google/resolve"
    ];

    for endpoint in endpoints {
        match query_doh(&client, endpoint, hostname).await {
            Ok(ip) => {
                log::info!("DoH çözümlendi ({}): {} → {}", endpoint, hostname, ip);
                return Ok(ip);
            }
            Err(e) => {
                log::debug!("DoH başarısız ({}): {} — {}", endpoint, hostname, e);
                continue;
            }
        }
    }

    // Fallback: Sistem DNS'i
    log::warn!("DoH başarısız, sistem DNS deneniyor: {}", hostname);
    match tokio::net::lookup_host(format!("{}:0", hostname)).await {
        Ok(mut addrs) => {
            if let Some(addr) = addrs.next() {
                log::info!("Sistem DNS çözümlendi: {} → {}", hostname, addr.ip());
                return Ok(addr.ip());
            }
            Err(format!("Sistem DNS: {} için kayıt bulunamadı", hostname))
        }
        Err(e) => Err(format!("DNS tamamen başarısız: {} — {}", hostname, e)),
    }
}

async fn query_doh(client: &Client, endpoint: &str, hostname: &str) -> Result<IpAddr, String> {
    let url = format!("{}?name={}&type=A", endpoint, hostname);
    let response = client
        .get(&url)
        .header("accept", "application/dns-json")
        .send()
        .await
        .map_err(|e| format!("HTTP hatası: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP HTTP {}", response.status()));
    }

    let doh_res: DoHResponse = response.json().await.map_err(|e| format!("JSON hatası: {}", e))?;

    if doh_res.status != 0 {
        return Err(format!("DoH durum kodu: {}", doh_res.status));
    }

    let answers = doh_res.answer.ok_or("Yanıt A kaydı içermiyor")?;

    for answer in answers {
        // type 1 == A kaydı (IPv4)
        if answer.record_type == 1 {
            if let Ok(ip) = answer.data.parse::<Ipv4Addr>() {
                return Ok(IpAddr::V4(ip));
            }
        }
    }

    Err("Geçerli bir IPv4 adresi bulunamadı".into())
}
