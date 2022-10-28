use mlua::prelude::*;
use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use tungstenite::{connect, Message};
use tungstenite::{protocol::WebSocket, stream::MaybeTlsStream};
use url::Url;

struct Client {
    messages: VecDeque<String>,
    socket: Arc<Mutex<Option<WebSocket<MaybeTlsStream<TcpStream>>>>>,
}

impl Client {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            socket: Arc::new(Mutex::new(None)),
        }
    }
    fn connect(&mut self, url: String) {
        let (socket, _response) = connect(Url::parse(&url).unwrap()).expect("Can't connect");
        let socket = Arc::new(Mutex::new(Some(socket)));
        self.socket = socket.clone();
        std::thread::spawn(move || loop {
            if let Some(socket) = socket.lock().unwrap().as_mut() {
                if let Ok(msg) = socket.read_message() {
                    if let Message::Text(msg) = msg {
                        println!("Received: {}", msg);
                    }
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
}

impl LuaUserData for Client {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("poll", |_, this, ()| Ok(this.messages.pop_front()));
        methods.add_method_mut("connect", |_, this, url: String| Ok(this.connect(url)));
        methods.add_method_mut("send", |_, this, text: String| Ok(this.send(text)));
        methods.add_method_mut("close", |_, this, ()| Ok(this.close()));
    }
}

fn new_client(_: &Lua, _: ()) -> LuaResult<Client> {
    Ok(Client::new())
}

fn sleep(_: &Lua, time: f64) -> LuaResult<()> {
    std::thread::sleep(std::time::Duration::from_millis((time * 1000.0) as u64));
    Ok(())
}

fn hello(_: &Lua, name: String) -> LuaResult<()> {
    println!("hello, {}!", name);
    Ok(())
}

#[mlua::lua_module]
fn my_module(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("hello", lua.create_function(hello)?)?;
    exports.set("newClient", lua.create_function(new_client)?)?;
    exports.set("sleep", lua.create_function(sleep)?)?;
    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_test() -> LuaResult<()> {
        let lua = Lua::new();
        lua.globals()
            .set("newClient", lua.create_function(new_client)?)?;
        lua.globals().set("sleep", lua.create_function(sleep)?)?;
        lua.load(
            r#"
            local client = newClient()
            print(client)
            client:connect("ws://127.0.0.1:8080/live")
            sleep(10)
        "#,
        )
        .exec()?;
        Ok(())
    }
}
