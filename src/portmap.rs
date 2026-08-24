//! Asks the router whether the swarm can dial in.
//!
//! librqbit requests the UPnP mapping and says nothing when the router ignores
//! it, which is the common case on carrier-issued gateways: UPnP ships off, the
//! request falls into the void, and the only symptom is a torrent that crawls.
//! So we ask the router directly what it has mapped.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use tokio::net::UdpSocket;

pub const PORT: u16 = 4240;

const UNKNOWN: u8 = 0;
const OPEN: u8 = 1;
const NO_MAPPING: u8 = 2;
const NO_ROUTER: u8 = 3;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Unknown,
    Open,
    NoMapping,
    NoRouter,
}

impl State {
    pub fn is_closed(self) -> bool {
        matches!(self, State::NoMapping | State::NoRouter)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            State::Unknown => "unknown",
            State::Open => "open",
            State::NoMapping => "no-mapping",
            State::NoRouter => "no-router",
        }
    }
}

pub fn state() -> State {
    match STATE.load(Ordering::Relaxed) {
        OPEN => State::Open,
        NO_MAPPING => State::NoMapping,
        NO_ROUTER => State::NoRouter,
        _ => State::Unknown,
    }
}

/// Rechecks forever, after giving librqbit time to place its mapping. The
/// router can be rebooted or have UPnP switched on while we run.
pub async fn watch() {
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let state = match probe().await {
            Ok(true) => OPEN,
            Ok(false) => NO_MAPPING,
            Err(_) => NO_ROUTER,
        };
        STATE.store(state, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_secs(280)).await;
    }
}

async fn probe() -> anyhow::Result<bool> {
    let location = discover().await?;
    let (service, control) = wan_service(&location).await?;
    Ok(has_mapping(&service, &control).await?)
}

/// SSDP M-SEARCH for the internet gateway, returning its description URL.
async fn discover() -> anyhow::Result<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let msg = concat!(
        "M-SEARCH * HTTP/1.1\r\n",
        "HOST:239.255.255.250:1900\r\n",
        "MAN:\"ssdp:discover\"\r\n",
        "MX:2\r\n",
        "ST:urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n"
    );
    for _ in 0..3 {
        socket.send_to(msg.as_bytes(), "239.255.255.250:1900").await?;
    }
    let mut buf = vec![0u8; 2048];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        let read = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await;
        let Ok(Ok((len, _))) = read else {
            anyhow::bail!("no gateway answered SSDP");
        };
        if let Some(location) = header(&String::from_utf8_lossy(&buf[..len]), "location:") {
            return Ok(location);
        }
    }
}

fn header(response: &str, name: &str) -> Option<String> {
    response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with(name))
        .map(|line| line[name.len()..].trim().to_string())
}

/// The WAN connection service that owns the port mapping table.
async fn wan_service(location: &str) -> anyhow::Result<(String, String)> {
    let xml = reqwest::Client::new().get(location).send().await?.error_for_status()?.text().await?;
    for block in xml.split("<service>").skip(1) {
        let Some(service) = tag(block, "serviceType") else { continue };
        if !service.contains("WANIPConnection") && !service.contains("WANPPPConnection") {
            continue;
        }
        let Some(path) = tag(block, "controlURL") else { continue };
        let control = url::Url::parse(location)?.join(&path)?.to_string();
        return Ok((service, control));
    }
    anyhow::bail!("gateway exposes no WAN connection service")
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim().to_string())
}

/// Walks the mapping table looking for our port. GetSpecificPortMappingEntry
/// would be one call, but it needs the external address the router chose, and
/// a home table is a handful of rows anyway.
async fn has_mapping(service: &str, control: &str) -> anyhow::Result<bool> {
    let client = reqwest::Client::new();
    for index in 0..64 {
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
             <u:GetGenericPortMappingEntry xmlns:u=\"{service}\">\
             <NewPortMappingIndex>{index}</NewPortMappingIndex>\
             </u:GetGenericPortMappingEntry></s:Body></s:Envelope>"
        );
        let response = client
            .post(control)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPAction", format!("\"{service}#GetGenericPortMappingEntry\""))
            .timeout(Duration::from_secs(5))
            .body(body)
            .send()
            .await?;
        // The table ends with a SOAP fault, which is how the walk terminates.
        if !response.status().is_success() {
            return Ok(false);
        }
        let xml = response.text().await?;
        if tag(&xml, "NewInternalPort").as_deref() == Some(&PORT.to_string()) {
            return Ok(true);
        }
    }
    Ok(false)
}
