# mlua + tungstenite

Learning multithreading with Rust. By the way, I wrote this library for love2d, which includes websocket client and json modules.

## Documents
- `newClient() -> Client`
- `Client:connect(url: string)`
- `Client:poll() -> string?`
- `Client:send(text: string)`
- `Client:close()`
- `jsonParse(value: string) -> table`
- `jsonStringify(value: table) -> string`

## Example
For the full example for love2d, see [test/main.lua](test/main.lua).

## Dependencies
- pkg-config
- love2d 11.4
