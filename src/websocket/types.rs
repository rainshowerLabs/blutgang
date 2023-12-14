use serde_json::Value;

// Used to either specify if its an incoming call or a subscription
#[derive(Debug)]
pub enum RequestResult {
    Call(Value),
    Subscription(Value),
}

impl From<RequestResult> for Value {
    fn from(req: RequestResult) -> Self {
        match req {
            RequestResult::Call(call) => call,
            RequestResult::Subscription(sub) => sub,
        }
    }
}

// Internal messages for the websocket manager
#[derive(Debug)]
pub enum WsconnMessage {
    Message(Value),
    Reconnect(),
}

impl From<WsconnMessage> for Value {
    fn from(msg: WsconnMessage) -> Self {
        match msg {
            WsconnMessage::Message(msg) => msg,
            WsconnMessage::Reconnect() => Value::Null,
        }
    }
}

// Errors to send to the health check when a WsConn fails
#[derive(Debug)]
pub enum WsChannelErr {
    Closed(usize),
}
