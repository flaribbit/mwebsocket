use mlua::prelude::*;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use tungstenite::{connect, Message};
use tungstenite::{protocol::WebSocket, stream::MaybeTlsStream};

struct Client {
    tx: Sender<String>,
    messages: Receiver<String>,
    socket: Arc<Mutex<Option<WebSocket<MaybeTlsStream<TcpStream>>>>>,
}

struct HttpResponse {
    status: i32,
    headers: HashMap<String, String>,
    body: String,
}

struct Promise {
    data: Arc<Mutex<Option<HttpResponse>>>,
}

const EVENT_PREFIX: &str = "@";

impl Client {
    fn new() -> Self {
        let (tx, rx) = channel::<String>();
        Self {
            tx,
            messages: rx,
            socket: Arc::new(Mutex::new(None)),
        }
    }
    fn connect(&mut self, url: String, headers: Option<Vec<[String; 2]>>) {
        let socket = self.socket.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = match headers {
                Some(headers) => connect_with_headers(url, headers),
                None => connect(url),
            };
            match res {
                Ok((s, _)) => *socket.lock().unwrap() = Some(s),
                Err(e) => {
                    push_error(&tx, &e.to_string());
                    push_event(&tx, "close");
                    return;
                }
            }
            push_event(&tx, "open");
            loop {
                if let Some(socket) = socket.lock().unwrap().as_mut() {
                    let close = check_message(&tx, socket.read());
                    if close {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            *socket.lock().unwrap() = None;
            push_event(&tx, "close");
        });
    }
    fn send(&mut self, text: String) {
        if let Some(socket) = self.socket.lock().unwrap().as_mut() {
            if let Err(e) = socket.write(Message::Text(text)) {
                push_error(&self.tx, &e.to_string());
            }
        }
    }
    fn close(&mut self) {
        if let Some(socket) = self.socket.lock().unwrap().as_mut() {
            if let Err(e) = socket.close(None) {
                push_error(&self.tx, &e.to_string());
            }
        }
    }
    fn poll(&mut self) -> Option<String> {
        self.messages.try_recv().ok()
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
    let uri: http::Uri = url.parse()?;
    let host = uri.host().ok_or(tungstenite::Error::Url(
        tungstenite::error::UrlError::EmptyHostName,
    ))?;
    let key = tungstenite::handshake::client::generate_key();
    let request = http::Request::builder()
        .uri(&uri)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", key);
    let request = headers
        .iter()
        .fold(request, |req, [k, v]| req.header(k, v))
        .body(())?;
    connect(request)
}

fn push_event(tx: &Sender<String>, event: &str) {
    tx.send(EVENT_PREFIX.to_string() + event).unwrap();
}

fn push_error(tx: &Sender<String>, error: &str) {
    push_event(tx, &("error: ".to_string() + error));
}

fn check_message(tx: &Sender<String>, incoming: tungstenite::Result<Message>) -> bool {
    match incoming {
        Ok(message) => {
            if let Message::Text(text) = message {
                if let Err(e) = tx.send(text) {
                    push_error(tx, &e.to_string());
                }
            }
            false
        }
        Err(tungstenite::Error::AlreadyClosed) => {
            push_error(tx, "Connection already closed");
            true
        }
        Err(tungstenite::Error::ConnectionClosed) => true,
        Err(error) => {
            push_error(tx, &error.to_string());
            false
        }
    }
}

fn http_get(_: &Lua, (url, headers): (String, Option<Vec<[String; 2]>>)) -> LuaResult<Promise> {
    let res = Promise {
        data: Arc::new(Mutex::new(None)),
    };
    let request = match headers {
        Some(headers) => headers
            .iter()
            .fold(minreq::get(&url), |r, [k, v]| r.with_header(k, v)),
        None => minreq::get(&url),
    };
    let pdata = res.data.clone();
    std::thread::spawn(move || {
        let response = request.send().unwrap();
        pdata.lock().unwrap().replace(HttpResponse {
            status: response.status_code,
            headers: response.headers.clone(),
            body: unsafe { String::from_utf8_unchecked(response.as_bytes().to_vec()) },
        });
    });
    Ok(res)
}

impl Promise {
    fn poll(&mut self) -> Option<HttpResponse> {
        self.data.lock().unwrap().take()
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

impl LuaUserData for Promise {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("poll", |lua, this, ()| {
            if let Some(response) = this.poll() {
                let mut ret = LuaMultiValue::new();
                ret.push_front(lua.to_value(&response.body)?);
                ret.push_front(lua.to_value(&response.status)?);
                ret.push_front(lua.to_value(&response.headers)?);
                Ok(ret)
            } else {
                Ok(LuaMultiValue::new())
            }
        });
    }
}

fn new_client(_: &Lua, _: ()) -> LuaResult<Client> {
    Ok(Client::new())
}

fn to_lua_error<E: ToString>(e: E) -> LuaError {
    LuaError::RuntimeError(e.to_string())
}

fn json_parse(lua: &Lua, text: String) -> LuaResult<LuaValue> {
    lua.to_value(&serde_json::from_str::<serde_json::Value>(&text).map_err(to_lua_error)?)
}

fn json_stringify(_: &Lua, value: LuaValue) -> LuaResult<String> {
    serde_json::to_string(&value).map_err(to_lua_error)
}

#[mlua::lua_module]
fn mwebsocket(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("newClient", lua.create_function(new_client)?)?;
    exports.set("jsonParse", lua.create_function(json_parse)?)?;
    exports.set("jsonStringify", lua.create_function(json_stringify)?)?;
    exports.set("get", lua.create_function(http_get)?)?;
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
