use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Table, Value};

use super::{BackendConfig, Config, ModelConfig, PoolOptions, Route};

pub(super) fn register ( lua: &Lua, state: Rc<RefCell<Config>> ) -> mlua::Result<Table> {

    let globals = lua.create_table()?;
    for name in ["set_listen", "set_upstream"] {
        let state = state.clone();
        globals.set(name, lua.create_function(move |_, value: String| {
            let address = value.parse().map_err(mlua::Error::external)?;
            if name == "set_listen" { state.borrow_mut().listen = address; }
            else { state.borrow_mut().upstream = address; }
            Ok(())
        })?)?;
    }
    let settings = state.clone();
    globals.set("set_model", lua.create_function(move |lua, value: Value| {
        if matches!(value, Value::String(_)) {
            settings.borrow_mut().model.mode = lua.from_value(value)?;
        } else {
            settings.borrow_mut().model = lua.from_value::<ModelConfig>(value)?;
        }
        Ok(())
    })?)?;
    for name in ["set_store", "set_default_upstream"] {
        let settings = state.clone();
        globals.set(name, lua.create_function(move |_, value: Value| {
            let value = match value {
                Value::Boolean(false) => None,
                Value::String(value) => Some(value.to_str()?.to_string()),
                _ => return Err(mlua::Error::RuntimeError(format!("{name} requires a string or false"))),
            };
            if name == "set_store" { settings.borrow_mut().store = value.map(Into::into); }
            else { settings.borrow_mut().default_pool = value; }
            Ok(())
        })?)?;
    }
    for name in ["set_queue", "set_persistence", "set_limits", "set_runtime", "set_tls", "set_preserve_host", "set_cache", "set_context", "set_identity", "set_telemetry", "set_control", "set_webhooks"] {
        let settings = state.clone();
        globals.set(name, lua.create_function(move |lua, value: Value| {
            let mut config = settings.borrow_mut();
            match name {
                "set_queue" => config.queue = lua.from_value(value)?,
                "set_persistence" => config.persistence = lua.from_value(value)?,
                "set_context" => config.context = lua.from_value(value)?,
                "set_cache" => config.cache = lua.from_value(value)?,
                "set_identity" => config.identity = lua.from_value(value)?,
                "set_telemetry" => config.telemetry = lua.from_value(value)?,
                "set_control" => config.control = lua.from_value(value)?,
                "set_webhooks" => config.webhooks = lua.from_value(value)?,
                "set_limits" => config.limits = lua.from_value(value)?,
                "set_runtime" => config.runtime = lua.from_value(value)?,
                "set_tls" => config.tls = if matches!(value, Value::Boolean(false)) { None } else { Some(lua.from_value(value)?) },
                _ => config.preserve_host = lua.from_value(value)?,
            }
            Ok(())
        })?)?;
    }
    let settings = state.clone();
    globals.set("add_upstream", lua.create_function(move |lua, (name, value): (String, Value)| {
        let backend = if let Value::String(address) = value {
            BackendConfig { address: address.to_str()?.parse().map_err(mlua::Error::external)?, ..BackendConfig::default() }
        } else {
            lua.from_value(value)?
        };
        settings.borrow_mut().pools.entry(name).or_default().backends.push(backend);
        Ok(())
    })?)?;
    let settings = state.clone();
    globals.set("set_balancer", lua.create_function(move |lua, (name, value): (String, Value)| {
        settings.borrow_mut().pools.entry(name).or_default().options = lua.from_value::<PoolOptions>(value)?;
        Ok(())
    })?)?;
    let settings = state.clone();
    globals.set("add_route", lua.create_function(move |lua, value: Value| {
        settings.borrow_mut().routes.push(lua.from_value::<Route>(value)?);
        Ok(())
    })?)?;
    globals.set("set_headers", lua.create_function(move |lua, (direction, value): (String, Value)| {
        match direction.as_str() {
            "request" => state.borrow_mut().request_headers = lua.from_value(value)?,
            "response" => state.borrow_mut().response_headers = lua.from_value(value)?,
            _ => return Err(mlua::Error::RuntimeError("Header direction must be request or response".into())),
        }
        Ok(())
    })?)?;

    Ok(globals)

}
