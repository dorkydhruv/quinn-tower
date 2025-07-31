use {
    crate::cloudflare::CloudflareKV,
    anyhow::Context,
    quinn::{ ServerConfig, crypto::rustls::QuicServerConfig },
    rustls::pki_types::{ CertificateDer, PrivateKeyDer, pem::PemObject },
    std::{
        net::{ IpAddr, Ipv4Addr, SocketAddr },
        path::Path,
        sync::Arc,
        time::{ SystemTime, UNIX_EPOCH },
    },
    tokio::{ fs, time::{ Duration, interval } },
    tracing::{ error, info, warn },
};

pub async fn init_sender(
    port: u16,
    tower_file_path: &Path,
    cert_path: &Path,
    key_path: &Path,
    cloudflare_token: Option<String>
) -> Result<(), anyhow::Error> {
    // Install crypto provider
    rustls::crypto::aws_lc_rs
        ::default_provider()
        .install_default()
        .map_err(|e| anyhow::anyhow!("Error installing default provider: {:?}", e))?;

    // Load certificates
    let cert = CertificateDer::from_pem_file(cert_path).context("Error reading certificate file")?;
    let key = PrivateKeyDer::from_pem_file(key_path).context("Error reading private key file")?;

    let mut server_crypto = rustls::ServerConfig
        ::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;

    server_crypto.alpn_protocols = vec![b"quinn-tower".to_vec()];

    let server_config = ServerConfig::with_crypto(
        Arc::new(QuicServerConfig::try_from(server_crypto)?)
    );

    // Start QUIC server
    let endpoint = quinn::Endpoint::server(
        server_config,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    )?;

    info!("QUIC sender started on port {}", port);

    // Setup Cloudflare KV if token provided
    let cloudflare_kv = if let Some(token) = cloudflare_token {
        match CloudflareKV::new(token) {
            Ok(kv) => {
                info!("Cloudflare KV backup enabled");
                Some(kv)
            }
            Err(e) => {
                warn!("Failed to setup Cloudflare KV: {}", e);
                None
            }
        }
    } else {
        None
    };

    let tower_path_clone = tower_file_path.to_owned();
    let quic_task = async move {
        while let Some(conn) = endpoint.accept().await {
            let tower_path = tower_path_clone.clone();
            tokio::spawn(async move {
                match conn.await {
                    Ok(connection) => {
                        info!("New client connected: {}", connection.remote_address());
                        if let Err(e) = handle_client_connection(connection, &tower_path).await {
                            error!("Error handling client: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to establish connection: {}", e);
                    }
                }
            });
        }
        Ok::<(), anyhow::Error>(())
    };

    let tower_path_clone = tower_file_path.to_owned();
    let backup_task = async move {
        if let Some(kv) = cloudflare_kv {
            let mut backup_interval = interval(Duration::from_secs(30));
            loop {
                backup_interval.tick().await;
                if let Err(e) = backup_to_cloudflare(&kv, &tower_path_clone).await {
                    warn!("Cloudflare backup failed: {}", e);
                }
            }
        } else {
            // No KV configured, just wait forever
            std::future::pending::<()>().await;
        }
        Ok::<(), anyhow::Error>(())
    };

    // Run both tasks concurrently
    tokio::try_join!(quic_task, backup_task)?;
    Ok(())
}

/// Handle a single client connection - send the tower file
async fn handle_client_connection(
    connection: quinn::Connection,
    tower_file_path: &Path
) -> Result<(), anyhow::Error> {
    let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

    let tower_data = fs::read(tower_file_path).await.context("Failed to read tower file")?;

    send_stream.write_all(&tower_data).await?;

    info!("Sent tower file ({} bytes) to {}", tower_data.len(), connection.remote_address());

    // Wait for acknowledgment
    let mut ack = [0u8; 1];
    if let Err(e) = recv_stream.read_exact(&mut ack).await {
        warn!("Failed to read acknowledgment: {}", e);
        return Ok(());
    }

    if ack[0] == 1 {
        info!("Client acknowledged successful transfer");
    } else {
        warn!("Client reported error receiving file");
    }

    send_stream.finish()?;

    Ok(())
}

/// Backup tower file to Cloudflare KV
async fn backup_to_cloudflare(
    kv: &CloudflareKV,
    tower_file_path: &Path
) -> Result<(), anyhow::Error> {
    let tower_data = fs
        ::read(tower_file_path).await
        .context("Failed to read tower file for backup")?;

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Store the raw tower data
    kv.write("tower_file", &tower_data).await?;

    // Store metadata separately for fast checks
    let metadata = format!(r#"{{"timestamp":{},"size":{}}}"#, timestamp, tower_data.len());
    kv.write("tower_metadata", metadata.as_bytes()).await?;

    info!("Backed up tower file to Cloudflare KV ({} bytes)", tower_data.len());
    Ok(())
}
