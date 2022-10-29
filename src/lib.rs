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

impl Client {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::new())),
            socket: Arc::new(Mutex::new(None)),
        }
    }
    fn connect(&mut self, url: String) {
        let (socket, _response) = connect(Url::parse(&url).unwrap()).expect("Can't connect");
        let socket = Arc::new(Mutex::new(Some(socket)));
        let messages = self.messages.clone();
        self.socket = socket.clone();
        messages.lock().unwrap().push_back("$open".to_string());
        std::thread::spawn(move || loop {
            if let Some(socket) = socket.lock().unwrap().as_mut() {
                if let Ok(msg) = socket.read_message() {
                    match msg {
                        Message::Text(text) => {
                            messages.lock().unwrap().push_back(text);
                        }
                        Message::Binary(_) => {}
                        _ => {}
                    }
                } else {
                    messages.lock().unwrap().push_back("$close".to_string());
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
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
    exports.set("json_parse", lua.create_function(json_parse)?)?;
    exports.set("json_stringify", lua.create_function(json_stringify)?)?;
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
