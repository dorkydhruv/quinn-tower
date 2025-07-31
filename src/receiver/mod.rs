mod certficate_verifier;
use {
    crate::cloudflare::CloudflareKV,
    anyhow::Context,
    certficate_verifier::InsecureCertificateVerifier,
    quinn::{ClientConfig, Endpoint},
    std::{
        net::SocketAddr,
        path::Path,
        sync::Arc,
        time::{ SystemTime, UNIX_EPOCH},
    },
    tokio::fs,
    tracing::{error, info, warn},
};

pub async fn init_receiver(
    sender_address: &str,
    output_path: &Path,
    cloudflare_token: Option<String>,
    max_staleness_seconds: u64,
) -> Result<(), anyhow::Error> {
    // Install crypto provider
    rustls::crypto::aws_lc_rs
        ::default_provider()
        .install_default()
        .map_err(|e| anyhow::anyhow!("Error installing default provider: {:?}", e))?;

    let quic_future = 
        try_quic_connection(sender_address, output_path);

    let _kv_future = 
        try_kv_fallback(
            cloudflare_token.as_ref(),
            output_path,
            max_staleness_seconds,
        );

    quic_future.await?;
    Ok(())
}

/// Attempt to receive tower file via QUIC
async fn try_quic_connection(
    sender_address: &str,
    output_path: &Path,
) -> Result<(), anyhow::Error> {
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;

    let mut tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty()) // We provide our own verifier
        .with_no_client_auth();
    tls_config
        .dangerous()
        .set_certificate_verifier(Arc::new(InsecureCertificateVerifier));
    tls_config.alpn_protocols = vec![b"quinn-tower".to_vec()];

    let client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
    ));
    endpoint.set_default_client_config(client_config);

    let server_addr: SocketAddr = sender_address.parse().context("Invalid sender address")?;

    let connection = endpoint.connect(server_addr, "localhost")?.await?;
    info!("Connected to sender via QUIC");

    let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

    let mut file_data = Vec::new();
    recv_stream.read_exact(&mut file_data).await?;

    fs::write(output_path, &file_data)
        .await
        .context("Failed to write tower file")?;

    send_stream.write_all(&[1u8]).await?;
    Ok(())
}

/// Fallback: try to get tower file from Cloudflare KV
async fn try_kv_fallback(
    cloudflare_token: Option<&String>,
    output_path: &Path,
    max_staleness_seconds: u64,
) -> Result<(), anyhow::Error> {
    let token = cloudflare_token.ok_or_else(|| anyhow::anyhow!("No Cloudflare token provided"))?;

    let cloudflare = CloudflareKV::new(token.clone())?;

    // Check metadata first for freshness
    match cloudflare.read("tower_metadata").await {
        Ok(metadata_bytes) => {
            let metadata_str =
                String::from_utf8(metadata_bytes).context("Invalid metadata encoding")?;

            let timestamp = extract_timestamp_from_json(&metadata_str)?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let age_seconds = now - timestamp;

            if age_seconds > max_staleness_seconds {
                info!(
                    "KV data too stale ({} seconds), starting fresh",
                    age_seconds
                );
                return Ok(());
            }

            info!(
                "KV data is {} seconds old, within acceptable range",
                age_seconds
            );
        }
        Err(_) => {
            warn!("No metadata found in KV, trying to get file anyway");
        }
    }

    // Get the actual tower file
    match cloudflare.read("tower_file").await {
        Ok(tower_data) => {
            fs::write(output_path, &tower_data)
                .await
                .context("Failed to write tower file from KV")?;

            info!(
                "Successfully retrieved tower file from Cloudflare KV ({} bytes)",
                tower_data.len()
            );
            Ok(())
        }
        Err(e) => {
            error!("Failed to retrieve tower file from KV: {}", e);
            Err(e.into())
        }
    }
}

fn extract_timestamp_from_json(json_str: &str) -> Result<u64, anyhow::Error> {
    if let Some(start) = json_str.find("\"timestamp\":") {
        let after_colon = &json_str[start + 12..];
        if let Some(end) = after_colon.find(&[',', '}'][..]) {
            let timestamp_str = &after_colon[..end];
            timestamp_str
                .parse::<u64>()
                .context("Failed to parse timestamp")
        } else {
            Err(anyhow::anyhow!("Malformed JSON: no timestamp end"))
        }
    } else {
        Err(anyhow::anyhow!("No timestamp found in metadata"))
    }
}
