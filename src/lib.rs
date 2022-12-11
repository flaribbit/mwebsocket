use mlua::prelude::*;
use mlua::LuaSerdeExt;
use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use tungstenite::{connect, Message};
use tungstenite::{protocol::WebSocket, stream::MaybeTlsStream};

struct Client {
    messages: Arc<Mutex<VecDeque<String>>>,
    socket: Arc<Mutex<Option<WebSocket<MaybeTlsStream<TcpStream>>>>>,
}

const EVENT_PREFIX: &str = "@";

impl Client {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::new())),
            socket: Arc::new(Mutex::new(None)),
        }
    }
    fn connect(&mut self, url: String, headers: Option<Vec<[String; 2]>>) {
        let socket = self.socket.clone();
        let messages = self.messages.clone();
        std::thread::spawn(move || {
            let res = match headers {
                Some(headers) => connect_with_headers(url, headers),
                None => connect(url),
            };
            match res {
                Ok((s, _)) => *socket.lock().unwrap() = Some(s),
                Err(e) => {
                    push_event(&messages, &format!("error: {e}"));
                    push_event(&messages, "close");
                    return;
                }
            }
            push_event(&messages, "open");
            loop {
                if let Some(socket) = socket.lock().unwrap().as_mut() {
                    let close = check_message(&messages, socket.read_message());
                    if close {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            *socket.lock().unwrap() = None;
            push_event(&messages, "close");
        });
    }
    fn send(&mut self, text: String) {
        if let Some(socket) = self.socket.lock().unwrap().as_mut() {
            socket.write_message(Message::Text(text)).unwrap();
        }
    }
    fn close(&mut self) {
        if let Some(socket) = self.socket.lock().unwrap().as_mut() {
            socket.close(None).unwrap();
        }
    }
    fn poll(&mut self) -> Option<String> {
        self.messages.lock().unwrap().pop_front()
    }
}

fn connect_with_headers(
    url: String,
    headers: Vec<[String; 2]>,
) -> Result<
    (
        WebSocket<MaybeTlsStream<TcpStream>>,
        tungstenite::handshake::client::Response,
    ),
    tungstenite::Error,
> {
    let uri: http::Uri = url.parse().unwrap();
    let key = tungstenite::handshake::client::generate_key();
    let request = http::Request::builder()
        .uri(url)
        .header("Host", uri.host().unwrap())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", key);
    let request = headers
        .iter()
        .fold(request, |req, [k, v]| req.header(k, v))
        .body(())
        .unwrap();
    connect(request)
}

fn push_event(messages: &Arc<Mutex<VecDeque<String>>>, event: &str) {
    messages
        .lock()
        .unwrap()
        .push_back(EVENT_PREFIX.to_string() + event);
}

fn check_message(
    messages: &Arc<Mutex<VecDeque<String>>>,
    incoming: tungstenite::Result<Message>,
) -> bool {
    match incoming {
        Ok(message) => {
            if let Message::Text(text) = message {
                messages.lock().unwrap().push_back(text);
            }
            false
        }
        Err(tungstenite::Error::AlreadyClosed) => {
            push_event(messages, "error: Connection already closed");
            true
        }
        Err(tungstenite::Error::ConnectionClosed) => true,
        Err(error) => {
            push_event(messages, &format!("error: {error}"));
            false
        }
    }
}

impl LuaUserData for Client {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("poll", |_, this, ()| Ok(this.poll()));
        methods.add_method_mut("connect", |_, this, (url, headers)| {
            this.connect(url, headers);
            Ok(())
        });
        methods.add_method_mut("send", |_, this, text: String| {
            this.send(text);
            Ok(())
        });
        methods.add_method_mut("close", |_, this, ()| {
            this.close();
            Ok(())
        });
    }
}

fn new_client(_: &Lua, _: ()) -> LuaResult<Client> {
    Ok(Client::new())
}

fn json_parse(lua: &Lua, text: String) -> LuaResult<LuaValue> {
    lua.to_value(&serde_json::from_str::<serde_json::Value>(&text).unwrap())
}

fn json_stringify(_: &Lua, value: LuaValue) -> LuaResult<String> {
    Ok(serde_json::to_string(&value).unwrap())
}

fn sleep(_: &Lua, time: f64) -> LuaResult<()> {
    std::thread::sleep(std::time::Duration::from_millis((time * 1000.0) as u64));
    Ok(())
}

#[mlua::lua_module]
fn mwebsocket(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("newClient", lua.create_function(new_client)?)?;
    exports.set("sleep", lua.create_function(sleep)?)?;
    exports.set("jsonParse", lua.create_function(json_parse)?)?;
    exports.set("jsonStringify", lua.create_function(json_stringify)?)?;
    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_test() -> LuaResult<()> {
        let lua = Lua::new();
        lua.globals().set("lib", mwebsocket(&lua)?)?;
        lua.load(
            r#"
            local client = lib.newClient()
            print(client)
            client:connect("ws://127.0.0.1:8080/live")
            while true do
                while true do
                    local msg = client:poll()
                    if not msg then break end
                    print('message:', msg)
                end
                lib.sleep(0.1)
            end
        "#,
        )
        .eval()?;
        Ok(())
    }
}
