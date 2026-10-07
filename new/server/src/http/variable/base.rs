use std::borrow::Cow;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;

use bytes::Bytes;
use maxminddb::PathElement;
use regex::bytes::Regex;

use crate::core::error::{AppError, AppResult};
use crate::http::key::{HashKey, Hint};
use super::arch::{Catalog, Check, Derived, Keyval, Kind, Recipe, Scalar, Source, Step, Subject, Test, Zone};

static ZONES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<Box<str>, Zone>>> = std::sync::OnceLock::new();

const DEPTH: usize = 6;

impl Keyval {

    pub fn zone ( name: &str, create: bool ) -> Option<Zone> {

        let mut zones = ZONES.get_or_init(Default::default).lock().ok()?;

        match ( zones.get(name), create ) {
            ( Some(zone), _ ) => Some(zone.clone()),
            ( None, true ) => { let zone = Zone::default(); zones.insert(name.into(), zone.clone()); Some(zone) }
            ( None, false ) => None,
        }

    }

    pub fn set ( zone: &Zone, key: &[u8], value: &[u8] ) {

        if let Ok(mut held) = zone.write() { held.insert(key.into(), Bytes::copy_from_slice(value)); }

    }

    pub fn remove ( zone: &Zone, key: Option<&[u8]> ) {

        if let Ok(mut held) = zone.write() { match key { Some(key) => { held.remove(key); } None => held.clear() } }

    }

    pub fn list ( zone: &Zone ) -> Vec<( String, String )> {

        zone.read().map_or_else(|_| Vec::new(), |held| held.iter().map(|( key, value )| ( String::from_utf8_lossy(key).into_owned(), String::from_utf8_lossy(value).into_owned() )).collect())

    }

}

impl Catalog {

    pub fn compile ( recipes: &BTreeMap<String, Recipe> ) -> AppResult<Self> {

        let list = recipes.iter().map(|( name, recipe )| Derived::compile(name, recipe)).collect::<AppResult<Vec<_>>>()?;
        let located = list.iter().any(|derived| matches!(derived.source, Source::Geo { .. } | Source::Mmdb { .. } | Source::Map { key: HashKey::Ip, .. } | Source::Split { key: HashKey::Ip, .. }));

        Ok(Self { list, located })

    }

    pub fn is_empty ( &self ) -> bool {

        self.list.is_empty()

    }

    pub fn index ( &self, name: &str ) -> Option<usize> {

        self.list.iter().position(|derived| &*derived.name == name)

    }

    pub fn value ( &self, index: usize, hint: Hint<'_> ) -> Cow<'_, [u8]> {

        self.list.get(index).map_or(Cow::Borrowed(b"".as_slice()), |derived| derived.value(hint))

    }

    pub fn capture ( &self, hint: Hint<'_> ) -> Vec<Bytes> {

        self.list.iter().map(|derived| Bytes::copy_from_slice(&derived.value(hint))).collect()

    }

}

impl Derived {

    fn compile ( name: &str, recipe: &Recipe ) -> AppResult<Self> {

        let fail = |message: String| AppError::config("variable", format!("`{name}`: {message}"));
        let key = |fallback: Option<HashKey>| match ( recipe.from.is_empty(), fallback ) {
            ( true, Some(fallback) ) => Ok(fallback),
            ( true, None ) => Err(fail("needs `from`".to_string())),
            ( false, _ ) => HashKey::compile(Some(&recipe.from), HashKey::Ip).map_err(|error| fail(error.to_string())),
        };

        let source = match recipe.kind {
            Kind::Map => {

                let mut exact = std::collections::HashMap::new();
                let mut patterns = Vec::new();

                for ( expected, value ) in &recipe.values {

                    match expected.strip_prefix('~') {
                        Some(pattern) => patterns.push(( Regex::new(pattern).map_err(|error| fail(format!("pattern `{pattern}`: {error}")))?, Bytes::copy_from_slice(value.as_bytes()) )),
                        None => { exact.insert(expected.as_bytes().into(), Bytes::copy_from_slice(value.as_bytes())); }
                    }

                }

                Source::Map { key: key(None)?, exact, patterns }

            }
            Kind::Geo => {

                let mut nets = recipe.values.iter().map(|( net, value )| Ok(( net.parse::<ipnet::IpNet>().or_else(|_| net.parse::<std::net::IpAddr>().map(ipnet::IpNet::from)).map_err(|_| fail(format!("`{net}` is not an address or a network")))?, Bytes::copy_from_slice(value.as_bytes()) ))).collect::<AppResult<Vec<_>>>()?;

                nets.sort_by_key(|( net, _ )| std::cmp::Reverse(net.prefix_len()));

                Source::Geo { nets }

            }
            Kind::Split => {

                let mut total = 0u64;
                let buckets: Vec<( u64, Bytes )> = recipe.buckets.iter().filter(|( _, share )| **share > 0).map(|( value, share )| { total += u64::from(*share); ( total, Bytes::copy_from_slice(value.as_bytes()) ) }).collect();

                if buckets.is_empty() { return Err(fail("needs at least one bucket with a share above zero".to_string())); }

                Source::Split { key: key(Some(HashKey::Ip))?, buckets, total }

            }
            Kind::Keyval => Source::Keyval { key: key(None)?, zone: Keyval::zone(name, true).ok_or_else(|| fail("the key-value store is unavailable".to_string()))? },
            Kind::Mmdb => {

                let path = recipe.path.as_ref().ok_or_else(|| fail("needs `path` to an .mmdb database".to_string()))?;
                let reader = maxminddb::Reader::open_readfile(path).map_err(|error| fail(format!("{}: {error}", path.display())))?;
                let steps: Box<[Step]> = recipe.field.split('.').filter(|part| !part.is_empty()).map(|part| part.parse::<usize>().map_or_else(|_| Step::Key(part.into()), Step::Index)).collect();

                if steps.is_empty() || steps.len() > DEPTH { return Err(fail(format!("`field` takes 1 to {DEPTH} dot-separated steps, such as `country.iso_code`"))); }

                Source::Mmdb { reader: Arc::new(reader), steps }

            }
        };

        Ok(Self { name: name.into(), source, fallback: Bytes::copy_from_slice(recipe.default.as_bytes()) })

    }

    fn value ( &self, hint: Hint<'_> ) -> Cow<'_, [u8]> {

        let found: Option<&Bytes> = match &self.source {
            Source::Map { key, exact, patterns } => key.material(hint, |bytes| exact.get(bytes).or_else(|| patterns.iter().find(|( pattern, _ )| pattern.is_match(bytes)).map(|( _, value )| value))).flatten(),
            Source::Geo { nets } => nets.iter().find(|( net, _ )| net.contains(&hint.ip)).map(|( _, value )| value),
            Source::Split { key, buckets, total } => { let slot = key.digest(hint) % total.max(&1); buckets.iter().find(|( edge, _ )| slot < *edge).map(|( _, value )| value) }
            Source::Mmdb { reader, steps } => return Self::locate(reader, steps, hint.ip).unwrap_or(Cow::Borrowed(&self.fallback[..])),
            Source::Keyval { key, zone } => return key.material(hint, |bytes| zone.read().ok().and_then(|held| held.get(bytes).cloned())).flatten().map_or(Cow::Borrowed(&self.fallback[..]), |value| Cow::Owned(value.to_vec())),
        };

        Cow::Borrowed(&found.unwrap_or(&self.fallback)[..])

    }

    fn locate <'r> ( reader: &'r maxminddb::Reader<Vec<u8>>, steps: &[Step], ip: IpAddr ) -> Option<Cow<'r, [u8]>> {

        let mut path: [PathElement<'_>; DEPTH] = std::array::from_fn(|_| PathElement::Index(0));

        for ( slot, step ) in path.iter_mut().zip(steps) {

            *slot = match step { Step::Key(key) => PathElement::Key(key), Step::Index(index) => PathElement::Index(*index) };

        }

        Some(match reader.lookup(ip).ok()?.decode_path::<Scalar<'r>>(&path[..steps.len()]).ok()?? {
            Scalar::Text(text) => Cow::Borrowed(text.as_bytes()),
            Scalar::Unsigned(number) => Cow::Owned(number.to_string().into_bytes()),
            Scalar::Signed(number) => Cow::Owned(number.to_string().into_bytes()),
            Scalar::Float(number) => Cow::Owned(number.to_string().into_bytes()),
            Scalar::Flag(flag) => Cow::Borrowed(if flag { b"1".as_slice() } else { b"0".as_slice() }),
        })

    }

}

impl Check {

    pub fn compile ( subject: Subject, expected: &str ) -> AppResult<Self> {

        let ( expected, negate ) = expected.strip_prefix('!').map_or(( expected, false ), |rest| ( rest, true ));

        let test = match ( expected, expected.strip_prefix('~') ) {
            ( "*", _ ) => Test::Present,
            ( _, Some(pattern) ) => Test::Pattern(Regex::new(pattern).map_err(|error| AppError::config("add_route", format!("pattern `{pattern}`: {error}")))?),
            ( _, None ) => Test::Exact(expected.as_bytes().into()),
        };

        Ok(Self { subject, test, negate })

    }

    pub fn passes ( &self, hint: Hint<'_>, catalog: &Catalog ) -> bool {

        let derived;

        let value: Option<&[u8]> = match &self.subject {
            Subject::Header(name) => hint.headers.get(name).map(|value| value.as_bytes()),
            Subject::Query(name) => HashKey::query(hint.uri, name),
            Subject::Derived(index) => { derived = catalog.value(*index, hint); Some(&*derived).filter(|value| !value.is_empty()) }
        };

        let matched = match ( &self.test, value ) {
            ( _, None ) => false,
            ( Test::Present, Some(_) ) => true,
            ( Test::Exact(expected), Some(value) ) => **expected == *value,
            ( Test::Pattern(pattern), Some(value) ) => pattern.is_match(value),
        };

        matched != self.negate

    }

}
