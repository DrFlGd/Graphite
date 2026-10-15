//! `graphite-relay`: run with `RELAY_DATA` (folder for the store, default
//! `./relay-data`), `RELAY_ADDR` (default `0.0.0.0:7474`) and
//! `RELAY_MAX_MB` (largest object accepted, default 1024).

use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let data = std::env::var_os("RELAY_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("relay-data"));
    let addr: SocketAddr = std::env::var("RELAY_ADDR")
        .ok()
        .and_then(|a| a.parse().ok())
        .unwrap_or_else(|| "0.0.0.0:7474".parse().expect("valid"));
    let max_mb: usize = std::env::var("RELAY_MAX_MB")
        .ok()
        .and_then(|m| m.parse().ok())
        .unwrap_or(1024);
    let relay = graphite_relay::Relay::open(&data)?;
    graphite_relay::serve(relay, addr, max_mb * 1024 * 1024).await
}
