use std::cell::Cell;
use std::rc::Rc;

use bytes::Bytes;
use http::StatusCode;
use http::header::{CONNECTION, CONTENT_LENGTH, COOKIE, HeaderMap, HeaderName, HeaderValue, TRANSFER_ENCODING, UPGRADE};
use mlua::{HookTriggers, LuaOptions, StdLib, Table, Value, VmState};

use crate::config::{Config, HookSpec};
use crate::config::dsl::Dsl;
use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::parse::Lua;
use crate::http::body::Body;
use crate::http::response::Res;
use super::arch::{Hooks, View};

const MEMORY: usize = 16 * 1_048_576;
const STRIDE: u32 = 1_000;
const STEPS: u32 = 2_000;

type Before = Vec<( HeaderName, Vec<u8> )>;

impl Hooks {

    pub fn load ( spec: &HookSpec ) -> AppResult<Self> {

        let lua = mlua::Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH, LuaOptions::default()).or_fail("cannot initialize lua")?;
        let spent = Rc::new(Cell::new(0u32));
        let meter = spent.clone();

        lua.set_memory_limit(MEMORY).or_fail("cannot limit lua memory")?;

        lua.set_hook(HookTriggers::new().every_nth_instruction(STRIDE), move |_, _| {

            meter.set(meter.get() + 1);

            if meter.get() > STEPS { Err(mlua::Error::RuntimeError("the hook ran past its instruction budget".into())) } else { Ok(VmState::Continue) }

        }).or_fail("cannot set lua instruction budget")?;

        let globals = Dsl::register(&lua, Config::default(), &spec.base).or_fail("cannot register configuration functions")?;

        Lua::run(&lua, &spec.source, &spec.name, globals)?;

        let request = lua.named_registry_value::<Option<mlua::Function>>("on_request").or_fail("cannot read on_request")?;
        let response = lua.named_registry_value::<Option<mlua::Function>>("on_response").or_fail("cannot read on_response")?;

        Ok(Self { lua, request, response, spent })

    }

    pub fn request ( &self, view: View<'_>, headers: &mut HeaderMap ) -> AppResult<Option<Res<Body>>> {

        let Some(hook) = &self.request else { return Ok(None); };
        let fail = |error: mlua::Error| AppError::http(500, format!("on_request: {error}"));

        self.spent.set(0);

        let subject = self.subject(view).map_err(fail)?;
        let ( table, before ) = self.table(headers).map_err(fail)?;

        subject.set("headers", table.clone()).map_err(fail)?;

        let outcome: Value = hook.call(subject).map_err(fail)?;

        Self::apply(&table, &before, headers).map_err(fail)?;

        match outcome {
            Value::Table(reply) => Self::reply(&reply).map(Some).map_err(fail),
            _ => Ok(None),
        }

    }

    pub fn response ( &self, view: View<'_>, response: &mut Res<Body> ) -> AppResult<()> {

        let Some(hook) = &self.response else { return Ok(()); };
        let fail = |error: mlua::Error| AppError::http(500, format!("on_response: {error}"));

        self.spent.set(0);

        let subject = self.subject(view).map_err(fail)?;
        let answer = self.lua.create_table().map_err(fail)?;
        let ( table, before ) = self.table(response.headers()).map_err(fail)?;

        answer.set("status", response.status().as_u16()).map_err(fail)?;
        answer.set("headers", table.clone()).map_err(fail)?;

        hook.call::<()>(( subject, answer.clone() )).map_err(fail)?;

        Self::apply(&table, &before, response.headers_mut()).map_err(fail)?;

        if let Some(status) = answer.get::<Option<u16>>("status").map_err(fail)?.and_then(|code| StatusCode::from_u16(code).ok()).filter(|status| status.as_u16() >= 200) { *response.status_mut() = status; }

        Ok(())

    }

    fn subject ( &self, view: View<'_> ) -> mlua::Result<Table> {

        let subject = self.lua.create_table_with_capacity(0, 8)?;

        subject.set("method", view.method.as_str())?;
        subject.set("path", view.uri.path())?;
        subject.set("query", view.uri.query().unwrap_or(""))?;
        subject.set("host", self.lua.create_string(view.host.map_or(b"".as_slice(), |host| host.as_bytes()))?)?;
        subject.set("ip", view.ip.to_string())?;
        subject.set("scheme", if view.secure { "https" } else { "http" })?;

        Ok(subject)

    }

    fn table ( &self, headers: &HeaderMap ) -> mlua::Result<( Table, Before )> {

        let table = self.lua.create_table_with_capacity(0, headers.keys_len())?;
        let mut before = Vec::with_capacity(headers.keys_len());

        for name in headers.keys() {

            let joint: &[u8] = if name == COOKIE { b"; " } else { b", " };
            let mut joined = Vec::new();

            for value in headers.get_all(name) {

                if !joined.is_empty() { joined.extend_from_slice(joint); }

                joined.extend_from_slice(value.as_bytes());

            }

            table.set(name.as_str(), self.lua.create_string(&joined)?)?;
            before.push(( name.clone(), joined ));

        }

        Ok(( table, before ))

    }

    fn apply ( table: &Table, before: &Before, headers: &mut HeaderMap ) -> mlua::Result<()> {

        let mut kept: Vec<HeaderName> = Vec::with_capacity(before.len());

        for pair in table.pairs::<mlua::String, Value>() {

            let ( name, value ) = pair?;
            let Ok(name) = HeaderName::from_bytes(&name.as_bytes()) else { continue; };

            let text = match value {
                Value::String(text) => text.as_bytes().to_vec(),
                Value::Integer(number) => number.to_string().into_bytes(),
                Value::Number(number) => number.to_string().into_bytes(),
                _ => continue,
            };

            if !Self::framing(&name) && before.iter().find(|( known, _ )| *known == name).is_none_or(|( _, joined )| *joined != text) && let Ok(value) = HeaderValue::from_bytes(&text) { headers.insert(name.clone(), value); }

            kept.push(name);

        }

        for ( name, _ ) in before.iter().filter(|( name, _ )| !kept.contains(name) && !Self::framing(name)) { headers.remove(name); }

        Ok(())

    }

    fn framing ( name: &HeaderName ) -> bool {

        name == CONTENT_LENGTH || name == TRANSFER_ENCODING || name == CONNECTION || name == UPGRADE

    }

    fn reply ( reply: &Table ) -> mlua::Result<Res<Body>> {

        let status = reply.get::<Option<u16>>("status")?.and_then(|code| StatusCode::from_u16(code).ok()).filter(|status| status.as_u16() >= 200).unwrap_or(StatusCode::OK);
        let body = reply.get::<Option<mlua::String>>("body")?.map(|text| Bytes::copy_from_slice(&text.as_bytes())).unwrap_or_default();
        let mut response = Res::new(if body.is_empty() { Body::Empty } else { Body::Bytes(body) });

        *response.status_mut() = status;

        if let Some(headers) = reply.get::<Option<Table>>("headers")? {

            for pair in headers.pairs::<mlua::String, mlua::String>() {

                let ( name, value ) = pair?;

                if let ( Ok(name), Ok(value) ) = ( HeaderName::from_bytes(&name.as_bytes()), HeaderValue::from_bytes(&value.as_bytes()) ) && !Self::framing(&name) { response.headers_mut().insert(name, value); }

            }

        }

        Ok(response)

    }

}
