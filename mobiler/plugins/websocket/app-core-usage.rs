// Rust app-side usage of the websocket plugin (drop into shared/src/app.rs).
// Rides the streaming primitive: `cx.subscribe` opens the socket and delivers a Msg::Frame per
// incoming frame (no app-driven recv loop); `cx.unsubscribe` closes it.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    Connect,
    Frame(PluginResponse),
    SendPing,
    Sent(PluginResponse),
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
            // Open the socket and stream frames — one Msg::Frame per incoming frame, until close.
            Msg::Connect => {
                model.connected = true;
                cx.subscribe("ws", "websocket", "stream", "wss://echo.websocket.events", Msg::Frame);
            }
            Msg::Frame(resp) => {
                if resp.ok {
                    model.log.push(format!("recv: {}", resp.output));
                } else {
                    // output == "closed" → the socket dropped.
                    model.connected = false;
                    model.log.push("disconnected".into());
                }
            }
            // Send a frame while subscribed (request/response op against the same socket).
            Msg::SendPing => cx.plugin("websocket", "send", "ping", Msg::Sent),
            Msg::Sent(_) => model.log.push("sent: ping".into()),
            // Stop streaming + close the socket.
            Msg::Disconnect => {
                cx.unsubscribe("ws");
                model.connected = false;
            }
            Msg::Noop(_) => {}
        }
    }
}
