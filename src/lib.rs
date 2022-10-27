use mlua::prelude::*;
use std::collections::VecDeque;

struct Client {
    messages: VecDeque<String>,
}
impl Client {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
        }
    }
}

impl LuaUserData for Client {
    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("poll", |_, this, ()| Ok(this.messages.pop_front()));
        methods.add_method_mut("new", |_, _this, _url: String| Ok(Client::new()));
    }
}

fn hello(_: &Lua, name: String) -> LuaResult<()> {
    println!("hello, {}!", name);
    Ok(())
}

#[mlua::lua_module]
fn my_module(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("hello", lua.create_function(hello)?)?;
    exports.set("client", lua.create_userdata(Client::new())?)?;
    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() -> LuaResult<()> {
        let lua = Lua::new();
        lua.globals().set("lib", my_module(&lua).unwrap())?;
        lua.load("lib.hello('123')").eval()?;
        assert!(true);
        Ok(())
    }
}
