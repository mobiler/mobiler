// Rust app-side usage of the websocket plugin (drop into shared/src/app.rs).
// The streaming receive is modeled as a self-re-issuing `recv` loop: each frame's handler
// kicks off the next recv, until the socket closes (ok:false, output "closed").

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    Connect,
    WsOpen(PluginResponse),
    SendPing,
    Sent(PluginResponse),
    WsFrame(PluginResponse),
    Disconnect,
    Noop(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub connected: bool,
    pub log: Vec<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Connect => cx.plugin("websocket", "connect", "wss://echo.websocket.org", Msg::WsOpen),
            Msg::WsOpen(resp) => {
                model.connected = resp.ok;
                if resp.ok {
                    model.log.push("connected".into());
                    // Start the receive loop.
                    cx.plugin("websocket", "recv", "", Msg::WsFrame);
                } else {
                    model.log.push(format!("connect failed: {}", resp.output));
                }
            }
            Msg::SendPing => cx.plugin("websocket", "send", "ping", Msg::Sent),
            Msg::Sent(_) => model.log.push("sent: ping".into()),
            Msg::WsFrame(resp) => {
                if resp.ok {
                    model.log.push(format!("recv: {}", resp.output));
                    // Re-issue recv to keep streaming.
                    cx.plugin("websocket", "recv", "", Msg::WsFrame);
                } else {
                    // output == "closed" → stop the loop.
                    model.connected = false;
                    model.log.push("disconnected".into());
                }
            }
            Msg::Disconnect => cx.plugin("websocket", "close", "", Msg::Noop),
            Msg::Noop(_) => {}
        }
    }
}
