use futures::{
    SinkExt, StreamExt
};

use tokio_tungstenite::{
    tungstenite::Message,
    MaybeTlsStream,
    WebSocketStream,
    connect_async,
};

use tokio::net::TcpStream;

#[derive(Debug, Copy, Clone)]
pub enum CreateRemoveWs {
    Create,
    Remove,
}

#[derive(Debug, Copy, Clone)]
pub struct Wsreg {
    pub create_remove: CreateRemoveWs,
    pub id: Option<usize>,
}

pub struct WsConnection {
    pub url: String,
    pub index: usize,
    pub socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WsConnection {
    pub async fn new(url: String, index: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let (socket, _) = connect_async(url.clone()).await?;
        Ok(Self {
            url,
            index,
            socket,
        })
    }

    pub async fn send(&mut self, message: String) -> Result<(), Box<dyn std::error::Error>> {
        self.socket.send(Message::Text(message)).await?;
        Ok(())
    }

    pub async fn read(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        let message = self.socket.next().await.ok_or("No message received")??;
        Ok(message)
    }
}
