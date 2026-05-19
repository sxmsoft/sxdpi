use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

const DNS_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const DOH_TIMEOUT: Duration = Duration::from_secs(3);

// DNS cache: hostname -> (IPs, inserted_at)
static DNS_CACHE: OnceLock<RwLock<HashMap<String, (Vec<IpAddr>, Instant)>>> = OnceLock::new();
static DOH_CLIENT: OnceLock<Client> = OnceLock::new();

fn get_cache() -> &'static RwLock<HashMap<String, (Vec<IpAddr>, Instant)>> {
    DNS_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn get_client() -> &'static Client {
    DOH_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(DOH_TIMEOUT)
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .expect("valid DoH HTTP client")
    })
}

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

/// Resolve all usable IPs for a hostname.
///
/// CDN-backed services can return several addresses. Trying only the first one
/// makes Microsoft Store / Roblox style installers fail randomly when that
/// edge is slow or unreachable.
pub async fn resolve_all(hostname: &str) -> Result<Vec<IpAddr>, String> {
    let hostname = normalize_hostname(hostname);

    if let Ok(ip) = hostname.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }

    {
        let cache = get_cache().read().unwrap();
        if let Some((ips, time)) = cache.get(&hostname) {
            if time.elapsed() < DNS_CACHE_TTL && !ips.is_empty() {
                log::debug!("Cache DNS: {} -> {:?}", hostname, ips);
                return Ok(ips.clone());
            }
        }
    }

    let client = get_client();
    let endpoints = [
        "https://cloudflare-dns.com/dns-query",
        "https://dns.google/resolve",
    ];

    for endpoint in endpoints {
        match query_doh(client, endpoint, &hostname).await {
            Ok(ips) if !ips.is_empty() => {
                log::info!("DoH resolved ({}): {} -> {:?}", endpoint, hostname, ips);
                cache_result(&hostname, &ips);
                return Ok(ips);
            }
            Err(e) => {
                log::debug!("DoH failed ({}): {} - {}", endpoint, hostname, e);
            }
            Ok(_) => {}
        }
    }

    log::warn!("DoH failed, trying system DNS: {}", hostname);
    match resolve_system(&hostname).await {
        Ok(ips) if !ips.is_empty() => {
            log::info!("System DNS resolved: {} -> {:?}", hostname, ips);
            cache_result(&hostname, &ips);
            Ok(ips)
        }
        Ok(_) => Err(format!("System DNS: no records for {}", hostname)),
        Err(e) => Err(format!("DNS failed completely: {} - {}", hostname, e)),
    }
}

async fn query_doh(client: &Client, endpoint: &str, hostname: &str) -> Result<Vec<IpAddr>, String> {
    let response = client
        .get(endpoint)
        .query(&[("name", hostname), ("type", "A")])
        .header("accept", "application/dns-json")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let doh_res: DoHResponse = response
        .json()
        .await
        .map_err(|e| format!("JSON error: {}", e))?;

    if doh_res.status != 0 {
        return Err(format!("DoH status code: {}", doh_res.status));
    }

    let answers = doh_res.answer.ok_or("response has no A records")?;
    let mut ips = Vec::new();

    for answer in answers {
        if answer.record_type == 1 {
            if let Ok(ip) = answer.data.parse::<Ipv4Addr>() {
                push_unique(&mut ips, IpAddr::V4(ip));
            }
        }
    }

    if ips.is_empty() {
        Err("no valid IPv4 address found".into())
    } else {
        Ok(ips)
    }
}

async fn resolve_system(hostname: &str) -> Result<Vec<IpAddr>, String> {
    let addrs = tokio::net::lookup_host((hostname, 0))
        .await
        .map_err(|e| e.to_string())?;

    let mut ips = Vec::new();
    for addr in addrs {
        push_unique(&mut ips, addr.ip());
    }

    Ok(ips)
}

fn cache_result(hostname: &str, ips: &[IpAddr]) {
    if let Ok(mut cache) = get_cache().write() {
        cache.insert(hostname.to_string(), (ips.to_vec(), Instant::now()));
    }
}

fn normalize_hostname(hostname: &str) -> String {
    hostname
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn push_unique(ips: &mut Vec<IpAddr>, ip: IpAddr) {
    if !ips.contains(&ip) {
        ips.push(ip);
    }
}
