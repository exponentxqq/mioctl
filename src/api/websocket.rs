use futures_util::StreamExt;
use tokio::sync::mpsc;
use crate::api::client::MihomoClient;
use crate::api::types::{Traffic, Memory, Connection, LogEntry};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[allow(dead_code)]
impl MihomoClient {
    fn ws_url(&self, path: &str) -> String {
        self.base_url()
            .replace("http://", "ws://")
            .replace("https://", "wss://")
            + path
    }

    pub async fn traffic_stream(
        &self,
    ) -> Result<mpsc::Receiver<Traffic>, crate::api::error::ApiError> {
        let (tx, rx) = mpsc::channel::<Traffic>(64);
        let url = self.ws_url("/traffic");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(traffic) = serde_json::from_str::<Traffic>(&text) {
                        let _ = tx.send(traffic).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn connection_stream(
        &self,
    ) -> Result<mpsc::Receiver<Vec<Connection>>, crate::api::error::ApiError> {
        let (tx, rx) = mpsc::channel::<Vec<Connection>>(64);
        let url = self.ws_url("/connections");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(conns) = data.get("connections") {
                            if let Ok(connections) =
                                serde_json::from_value::<Vec<Connection>>(conns.clone())
                            {
                                let _ = tx.send(connections).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn log_stream(
        &self,
        level: Option<&str>,
    ) -> Result<mpsc::Receiver<LogEntry>, crate::api::error::ApiError> {
        let (tx, rx) = mpsc::channel::<LogEntry>(256);
        let path = if let Some(lvl) = level {
            format!("/logs?level={}", lvl)
        } else {
            "/logs".to_string()
        };
        let url = self.ws_url(&path);
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(entry) = serde_json::from_str::<LogEntry>(&text) {
                        let _ = tx.send(entry).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn memory_stream(
        &self,
    ) -> Result<mpsc::Receiver<Memory>, crate::api::error::ApiError> {
        let (tx, rx) = mpsc::channel::<Memory>(16);
        let url = self.ws_url("/memory");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(mem) = serde_json::from_str::<Memory>(&text) {
                        let _ = tx.send(mem).await;
                    }
                }
            }
        });

        Ok(rx)
    }
}
