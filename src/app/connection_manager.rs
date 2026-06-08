use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::Connection;

pub struct ConnectionManager;

impl ConnectionManager {
    pub async fn list(client: &MihomoClient) -> ApiResult<Vec<Connection>> {
        let resp = client.get_connections().await?;
        Ok(resp.connections)
    }

    pub async fn close_one(client: &MihomoClient, id: &str) -> ApiResult<()> {
        client.close_connection(id).await
    }

    pub async fn close_all(client: &MihomoClient) -> ApiResult<()> {
        client.close_all_connections().await
    }
}
