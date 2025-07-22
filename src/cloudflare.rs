use {
    anyhow::Context,
    cloudflare::{
        endpoints::workerskv::{
            read_key::ReadKey,
            write_key::{WriteKey, WriteKeyBody, WriteKeyParams},
        },
        framework::{
            Environment,
            auth::Credentials,
            client::{ClientConfig, async_api::Client},
        },
    },
    once_cell::sync::Lazy,
    std::{env, time::Instant},
};

// Load these once at startup, not on every request
static ACCOUNT_ID: Lazy<String> =
    Lazy::new(|| env::var("ACCOUNT_ID").expect("ACCOUNT_ID must be set"));
static NAMESPACE_ID: Lazy<String> =
    Lazy::new(|| env::var("NAMESPACE_ID").expect("NAMESPACE_ID must be set"));

pub struct CloudflareKV {
    client: Client,
    account_id: &'static str,
    namespace_id: &'static str,
    write_params: WriteKeyParams,
}
impl CloudflareKV {
    /// Create a new instance with user auth token
    pub fn new(token: impl Into<String>) -> Result<Self, anyhow::Error> {
        // parse token only once
        let creds = Credentials::UserAuthToken {
            token: token.into(),
        };
        let config = ClientConfig::default();
        let client = Client::new(creds, config, Environment::Production)
            .context("Error: Unable to create new Cloudflare client")?;

        Ok(Self {
            client,
            account_id: &*ACCOUNT_ID,
            namespace_id: &*NAMESPACE_ID,
            // reuse common write params (TTL = 5 minutes)
            write_params: WriteKeyParams {
                expiration_ttl: Some(300),
                ..Default::default()
            },
        })
    }
    /// Read a key from KV; returns raw bytes
    pub async fn read(&self, key: &str) -> Result<Vec<u8>, anyhow::Error> {
        let req = ReadKey {
            account_identifier: self.account_id,
            namespace_identifier: self.namespace_id,
            key,
        };
        let start = Instant::now();
        let value = self.client.request(&req).await?;
        let elapsed = start.elapsed();
        tracing::debug!("KV read key '{}' took: {:?}", key, elapsed);
        Ok(value)
    }

    /// Write a key to KV; accepts bytes slice
    pub async fn write(&self, key: &str, value: &[u8]) -> Result<(), anyhow::Error> {
        let req = WriteKey {
            key,
            account_identifier: self.account_id,
            namespace_identifier: self.namespace_id,
            params: self.write_params.clone(),
            body: WriteKeyBody::Value(value.to_vec()),
        };
        let start = Instant::now();
        self.client.request(&req).await?;
        let elapsed = start.elapsed();
        tracing::debug!("KV write key '{}' took: {:?}", key, elapsed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {
        super::CloudflareKV,
        dotenv::dotenv,
        std::{env, fs},
        tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt},
    };

    #[tokio::test]
    async fn test_kv_roundtrip() -> Result<(), anyhow::Error> {
        // Load .env once
        dotenv().ok();
        let stdout_layer = fmt::layer()
            .with_timer(fmt::time::UtcTime::rfc_3339()) // 2025-06-07T03:37:59Z
            .with_thread_names(true)
            .with_file(true)
            .with_line_number(true)
            .compact(); // concise one-liner
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(stdout_layer)
            .init();

        let api_key = env::var("CLOUDFLARE_API_KEY")?;
        let kv = CloudflareKV::new(api_key)?;
        let key = "test_key";
        let data = fs::read("temp-tower.bin").expect("Error: unable to read tower file");
        // write then read
        kv.write(key, &data).await?;
        let result = kv.read(key).await?;
        assert_eq!(result, data);
        Ok(())
    }
}
