use mlua::{HookTriggers, LuaOptions, StdLib, Table, Value, VmState};
use serde::de::DeserializeOwned;

use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::{Lua, Sandbox};

impl Default for Sandbox {

    fn default () -> Self {

        Self { source_bytes: 65_536, memory_bytes: 1_048_576, instructions: 10_000 }

    }

}

impl Lua {

    pub fn sandbox ( limits: Sandbox ) -> AppResult<mlua::Lua> {

        let lua = mlua::Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH, LuaOptions::default()).or_fail("cannot initialize lua")?;

        lua.set_memory_limit(limits.memory_bytes).or_fail("cannot limit lua memory")?;

        lua.set_hook(HookTriggers::new().every_nth_instruction(limits.instructions), |_, _| {

            Err::<VmState, _>(mlua::Error::RuntimeError("configuration instruction budget exceeded".into()))

        }).or_fail("cannot set lua instruction budget")?;

        Ok(lua)

    }

    pub fn share ( lua: &mlua::Lua, globals: &Table, names: &[&str] ) -> mlua::Result<()> {

        for name in names { globals.set(*name, lua.globals().get::<Value>(*name)?)?; }

        Ok(())

    }

    pub fn run ( lua: &mlua::Lua, source: &str, name: &str, globals: Table ) -> AppResult<()> {

        lua.load(source).set_name(name).set_environment(globals).exec().map_err(|error| AppError::parse("lua", Self::describe(&error)))

    }

    pub fn value <T: DeserializeOwned> ( lua: &mlua::Lua, what: &str, value: Value ) -> mlua::Result<T> {

        use mlua::LuaSerdeExt;

        lua.from_value(value).map_err(|error| mlua::Error::RuntimeError(format!("{what}: {}", Self::describe(&error))))

    }

    pub fn table <T: DeserializeOwned> ( lua: &mlua::Lua, what: &str, value: Value ) -> mlua::Result<T> {

        use mlua::LuaSerdeExt;

        match value {
            Value::Table(_) => lua.from_value(value).map_err(|error| mlua::Error::RuntimeError(format!("{what}: {}", Self::describe(&error)))),
            other => Err(mlua::Error::RuntimeError(format!("{what} expects a table, found {}", other.type_name()))),
        }

    }

    pub fn text ( what: &str, value: &Value ) -> mlua::Result<String> {

        match value {
            Value::String(text) => Ok(text.to_str()?.to_string()),
            other => Err(mlua::Error::RuntimeError(format!("{what} expects a string, found {}", other.type_name()))),
        }

    }

    pub fn fail ( message: impl Into<String> ) -> mlua::Error {

        mlua::Error::RuntimeError(message.into())

    }

    pub fn describe ( error: &mlua::Error ) -> String {

        match error {
            mlua::Error::RuntimeError(message) => message.clone(),
            mlua::Error::CallbackError { cause, .. } => Self::describe(cause),
            mlua::Error::SyntaxError { message, .. } => message.clone(),
            other => other.to_string(),
        }

    }

}
