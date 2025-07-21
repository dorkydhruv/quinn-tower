use {
    anyhow::Context,
    quinn::{ClientConfig, ServerConfig, crypto::rustls::QuicServerConfig},
    rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        str::FromStr,
        sync::Arc,
    },
};

pub async fn init_sender(client_ip: &str, port: u16) -> Result<(), anyhow::Error> {
    let _ = match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "Error:  installing default provider: {:?}",
            e
        )),
    };
    let cert =
        CertificateDer::from_pem_file("cert.pem").context("Error reading certificate file")?;
    let key = PrivateKeyDer::from_pem_file("key.pem").context("Error reading private key file")?;
    // let server_config = ServerConfig::with_single_cert(vec![cert], key)?;

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;

    // server_crypto.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    let server_config =
        ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));

    let server = quinn::Endpoint::server(
        server_config,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
    )?;

    let client_socket = SocketAddr::from_str(client_ip).context("Error: parsing client ip")?;

    let config = ClientConfig::with_platform_verifier();
    let connection = server
        .connect_with(config, client_socket, "server_name")
        .context("Error: unable to make connection client")?
        .await?;

    let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

    // recv_stream.read
    loop {
        tokio::select! {}
    }

    Ok(())
}
