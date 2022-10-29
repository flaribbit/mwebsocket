# mlua + tungstenite

Learning multithreading with Rust. By the way, I wrote this library for love2d, which includes websocket client and json modules.

## Documents
- `newClient() -> Client`
- `Client:connect(url: string)`
- `Client:poll() -> string?`
- `Client:send(text: string)`
- `Client:close()`
- `json_parse(value: string) -> table`
- `json_stringify(value: table) -> string`

## Example
```bash
cd test
lovec .
```

## Dependencies
- pkg-config
- love2d 11.4