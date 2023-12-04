use std::format;

use async_trait::async_trait;

pub struct Client {
    pub handle: ezsockets::Client<Self>,
}

#[async_trait]
impl ezsockets::ClientExt for Client {
    type Call = ();

    async fn on_text(&mut self, text: String) -> Result<(), ezsockets::Error> {
        println!("received message: {}", text);
        Ok(())
    }

    async fn on_binary(&mut self, bytes: Vec<u8>) -> Result<(), ezsockets::Error> {
        println!("received bytes: {:?}", bytes);
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), ezsockets::Error> {
        let () = call;
        println!("received call: {:?}", call);
        Ok(())
    }
}

// Open
// fn ws_conn_manager(
//     rpc_list: Arc<RwLock<Vec<Rpc>>>,
// ) -> () {

// }

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(call: String) -> Result<String, ezsockets::Error> {
    Ok(format!("Hello from blutgang!: {}", call))
}
