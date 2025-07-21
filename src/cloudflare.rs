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
    std::env,
};

pub fn create_client(token: String) -> Result<Client, anyhow::Error> {
    let creds = Credentials::UserAuthToken { token };
    let config = ClientConfig::default();
    let client = Client::new(creds, config, Environment::Production)
        .context("Error: Unable to create new cloudflare client")?;
    Ok(client)
}

pub async fn read_tower(client: &Client, key: &str) -> Result<Vec<u8>, anyhow::Error> {
    let account_id = env::var("ACCOUNT_ID")?;
    let namespace_id = env::var("NAMESPACE_ID")?;
    let read_key = ReadKey {
        account_identifier: account_id.as_str(),
        namespace_identifier: namespace_id.as_str(),
        key,
    };
    let response = client.request(&read_key).await?;
    Ok(response)
}

pub async fn write_tower(client: &Client, key: &str, value: Vec<u8>) -> Result<(), anyhow::Error> {
    let account_id = env::var("ACCOUNT_ID")?;
    let namespace_id = env::var("NAMESPACE_ID")?;
    let write_key = WriteKey {
        key,
        account_identifier: account_id.as_str(),
        namespace_identifier: namespace_id.as_str(),
        params: WriteKeyParams::default(),
        body: WriteKeyBody::Value(value),
    };
    let response = client.request(&write_key).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cloudflare::{create_client, read_tower, write_tower};

    #[test]
    pub fn test_connection() -> Result<(), anyhow::Error> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("CLOUDFLARE_API_KEY")?;
        let _client = match create_client(api_key) {
            Ok(client) => client,
            Err(err) => return Err(err),
        };
        Ok(())
    }

    #[tokio::test]
    pub async fn test_write_tower() -> Result<(), anyhow::Error> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("CLOUDFLARE_API_KEY")?;
        let client = match create_client(api_key) {
            Ok(client) => client,
            Err(err) => return Err(err),
        };
        let key = "test_key";
        let value = b"test_value".to_vec();
        match write_tower(&client, key, value).await {
            Ok(_) => Ok(()),
            Err(err) => Err(err),
        }
    }

    #[tokio::test]
    pub async fn test_read_tower() -> Result<(), anyhow::Error> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("CLOUDFLARE_API_KEY")?;
        let client = match create_client(api_key) {
            Ok(client) => client,
            Err(err) => return Err(err),
        };
        let key = "test_key";
        let r = match read_tower(&client, key).await {
            Ok(r) => r,
            Err(err) => return Err(err),
        };

        println!("Data {:?}", r);
        Ok(())
    }
}
