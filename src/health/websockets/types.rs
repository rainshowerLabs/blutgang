use futures::{
    SinkExt,
    StreamExt
};

use tokio_tungstenite::{
    tungstenite::{
        Message,
        Error,
        Result
    },
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
    pub socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WsConnection {
    pub async fn new(url: String) -> Result<Self, Error> {
        let (socket, _) = connect_async(url.clone()).await?;
        Ok(Self {
            url,
            socket,
        })
    }

    pub async fn send(&mut self, message: String) -> Result<(), Error> {
        self.socket.send(Message::Text(message)).await?;
        Ok(())
    }

    pub async fn read(&mut self) -> Result<Message, Box<dyn std::error::Error>> {
        let message = self.socket.next().await.ok_or("No message received")??;
        Ok(message)
    }
}
