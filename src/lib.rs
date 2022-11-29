use mlua::prelude::*;
use mlua::LuaSerdeExt;
use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use tungstenite::{connect, Message};
use tungstenite::{protocol::WebSocket, stream::MaybeTlsStream};
use url::Url;

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
    fn connect(&mut self, url: String) {
        let socket = self.socket.clone();
        let messages = self.messages.clone();
        std::thread::spawn(move || {
            let res = connect(Url::parse(&url).unwrap());
            if res.is_err() {
                push_event(&messages, "close: Unable to connect");
                return;
            }
            *socket.lock().unwrap() = Some(res.unwrap().0);
            push_event(&messages, "open");
            loop {
                if let Some(socket) = socket.lock().unwrap().as_mut() {
                    let close = ckeck_message(&messages, socket.read_message());
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

fn push_event(messages: &Arc<Mutex<VecDeque<String>>>, event: &str) {
    messages
        .lock()
        .unwrap()
        .push_back(EVENT_PREFIX.to_string() + event);
}

fn ckeck_message(
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
            push_event(&messages, "error: Connection already closed");
            true
        }
        Err(tungstenite::Error::ConnectionClosed) => true,
        Err(error) => {
            push_event(&messages, &format!("error: {error}"));
            false
        }
    }
}

impl LuaUserData for Client {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("poll", |_, this, ()| Ok(this.poll()));
        methods.add_method_mut("connect", |_, this, url: String| Ok(this.connect(url)));
        methods.add_method_mut("send", |_, this, text: String| Ok(this.send(text)));
        methods.add_method_mut("close", |_, this, ()| Ok(this.close()));
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
