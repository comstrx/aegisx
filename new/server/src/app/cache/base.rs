use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::{Bytes, BytesMut};
use http::header::{AGE, AUTHORIZATION, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, DATE, ETAG, EXPIRES, HeaderMap, HeaderName, HeaderValue, IF_RANGE, LAST_MODIFIED, RANGE, SET_COOKIE, TRANSFER_ENCODING, VARY};
use http::{Method, StatusCode, Uri};
use serde_json::{Value, json};

use crate::config::CacheConfig;
use crate::core::cache::{Shelf, Weigh, Weighted};
use crate::core::log::warn;
use crate::core::time::Clock;
use crate::http::body::{Body, Probe};
use crate::http::header::Header;
use crate::http::response::{Res, Response};
use tokio::sync::Semaphore;

use crate::config::base::consts::CACHE_PASS_MS;
use crate::core::rt::Rt;
use super::arch::{Ban, Claim, Fill, Ignore, Lookup, Store, Stored};

const X_CACHE: HeaderName = HeaderName::from_static("x-cache");
const CACHE_TAG: HeaderName = HeaderName::from_static("cache-tag");
const X_ACCEL_EXPIRES: HeaderName = HeaderName::from_static("x-accel-expires");
const SURROGATE_KEY: HeaderName = HeaderName::from_static("surrogate-key");
const BANS_MAX: usize = 256;
const LOCK_MS: u64 = 5_000;

impl Weigh for Arc<Stored> {

    fn weight ( &self ) -> u64 {

        self.body.len() as u64 + 512

    }

}

impl Store {

    pub fn open ( config: &CacheConfig ) -> Self {

        let mut valid = Vec::new();
        let mut any = None;

        for ( status, ttl ) in &config.valid_ms {

            match status.parse::<u16>() { Ok(code) => valid.push(( code, *ttl )), Err(_) => any = Some(*ttl) }

        }

        let ignored = |name: &str| config.ignore_headers.iter().any(|ignored| ignored.eq_ignore_ascii_case(name));

        Self {
            entries    : Weighted::new(config.items, config.capacity_bytes),
            max_object : config.max_object_bytes as usize,
            valid,
            any,
            stale_ms   : config.stale_ms,
            vary       : config.key_headers.iter().filter_map(|name| HeaderName::from_bytes(name.to_ascii_lowercase().as_bytes()).ok()).collect(),
            rescue     : config.stale_if_error,
            lock       : config.lock,
            ignore     : Ignore { control: ignored("cache-control"), expires: ignored("expires"), cookie: ignored("set-cookie"), vary: ignored("vary") },
            epoch      : AtomicU64::new(1),
            fills      : papaya::HashMap::new(),
            lock_ms    : if config.lock { config.lock_ms } else { 0 },
            shelf      : config.path.as_ref().filter(|_| config.enabled).and_then(|path| Shelf::open(path, config.disk_bytes, Clock::now_ms()).inspect_err(|error| warn!(%error, path = %path.display(), "disk cache is off")).ok()).map(Arc::new),
            hits       : Default::default(),
            misses     : Default::default(),
            stale      : Default::default(),
            filled     : Default::default(),
            bans       : Default::default(),
            banned     : Default::default(),
        }

    }

    pub fn cacheable ( method: &Method, headers: &HeaderMap ) -> bool {

        (method == Method::GET || method == Method::HEAD) && !headers.contains_key(AUTHORIZATION)

    }

    pub fn key ( &self, host: &str, uri: &Uri, headers: &HeaderMap ) -> Bytes {

        let target = uri.path_and_query().map_or("/", |target| target.as_str());
        let mut key = BytesMut::with_capacity(host.len() + target.len() + 1 + self.vary.len() * 24);

        key.extend_from_slice(host.as_bytes());
        key.extend_from_slice(b" ");
        key.extend_from_slice(target.as_bytes());

        for name in &self.vary {

            key.extend_from_slice(b"\n");
            key.extend_from_slice(headers.get(name).map_or(b"".as_slice(), |value| value.as_bytes()));

        }

        key.freeze()

    }

    pub fn lookup ( &self, key: &mut Bytes, headers: &HeaderMap, now_ms: u64 ) -> Lookup {

        let Some(mut entry) = self.entries.get(key) else { self.misses.fetch_add(1, Ordering::Relaxed); return Lookup::Miss; };

        if !entry.vary.is_empty() {

            *key = Self::variant(key, &entry, headers);

            let Some(found) = self.entries.get(key) else { self.misses.fetch_add(1, Ordering::Relaxed); return Lookup::Miss; };

            entry = found;

        }

        if self.banned.load(Ordering::Relaxed) > 0 && entry.status != 0 && self.barred(key, &entry) {

            self.purge(key);
            self.misses.fetch_add(1, Ordering::Relaxed);

            return Lookup::Miss;

        }

        if entry.status == 0 {

            if now_ms < entry.expires_ms { return Lookup::Pass; }

            self.entries.remove(key);
            self.misses.fetch_add(1, Ordering::Relaxed);

            return Lookup::Miss;

        }

        if now_ms < entry.expires_ms { self.hits.fetch_add(1, Ordering::Relaxed); return Lookup::Hit(entry); }

        if now_ms >= entry.expires_ms.saturating_add(self.stale_ms) {

            self.entries.remove(key);
            self.misses.fetch_add(1, Ordering::Relaxed);

            return Lookup::Miss;

        }

        if !self.lock { self.misses.fetch_add(1, Ordering::Relaxed); return Lookup::Miss; }

        let held = entry.refreshing.load(Ordering::Relaxed);

        if held > now_ms || entry.refreshing.compare_exchange(held, now_ms + LOCK_MS, Ordering::AcqRel, Ordering::Relaxed).is_err() {

            self.stale.fetch_add(1, Ordering::Relaxed);

            return Lookup::Stale(entry);

        }

        self.misses.fetch_add(1, Ordering::Relaxed);

        if entry.headers.contains_key(ETAG) || entry.headers.contains_key(LAST_MODIFIED) { return Lookup::Revalidate(entry); }

        Lookup::Refresh(entry)

    }

    pub fn renew ( &self, key: Bytes, entry: &Stored, fresh: &HeaderMap, head: bool, now_ms: u64 ) -> Res {

        let mut headers = entry.headers.clone();

        for name in [CACHE_CONTROL, EXPIRES, DATE, ETAG, LAST_MODIFIED] { if let Some(value) = fresh.get(&name).and_then(|value| HeaderValue::from_bytes(value.as_bytes()).ok()) { headers.insert(name, value); } }

        let ttl = self.storable(entry.status, &headers, now_ms);
        let renewed = Arc::new(Stored { status: entry.status, headers, body: entry.body.clone(), created_ms: now_ms, expires_ms: now_ms.saturating_add(ttl.unwrap_or(0)), refreshing: AtomicU64::new(0), vary: Box::default(), epoch: 0 });

        match ttl {
            Some(_) => { self.entries.insert(key, renewed.clone()); self.filled.fetch_add(1, Ordering::Relaxed); }
            None => { self.entries.remove(&key); }
        }

        let mut response = Self::respond(&renewed, if head { &Method::HEAD } else { &Method::GET }, &HeaderMap::new(), now_ms, false);

        response.headers_mut().insert(X_CACHE, HeaderValue::from_static("REVALIDATED"));

        response

    }

    pub fn storable ( &self, status: u16, headers: &HeaderMap, now_ms: u64 ) -> Option<u64> {

        if !matches!(status, 200 | 203 | 204 | 301 | 308 | 404 | 405 | 410 | 414 | 501) { return None; }

        if (headers.contains_key(SET_COOKIE) && !self.ignore.cookie) || headers.contains_key(CONTENT_RANGE) { return None; }

        if headers.get(CONTENT_LENGTH).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<usize>().ok()).is_some_and(|length| length > self.max_object) { return None; }

        if let Some(accel) = headers.get(X_ACCEL_EXPIRES).and_then(|value| value.to_str().ok()).map(str::trim) {

            let ttl = match accel.strip_prefix('@') {
                Some(moment) => moment.parse::<u64>().ok().map(|at| at.saturating_mul(1_000).saturating_sub(now_ms)),
                None => accel.parse::<u64>().ok().map(|seconds| seconds.saturating_mul(1_000)),
            };

            if let Some(ttl) = ttl { return (ttl > 0).then_some(ttl); }

        }

        let mut directive = None;

        for value in headers.get_all(CACHE_CONTROL).iter().filter(|_| !self.ignore.control) {

            for token in value.to_str().unwrap_or("").split(',') {

                let token = token.trim();
                let lower = token.to_ascii_lowercase();

                if matches!(lower.as_str(), "no-store" | "no-cache" | "private") { return None; }

                if let Some(seconds) = lower.strip_prefix("s-maxage=") && let Ok(seconds) = seconds.trim().parse::<u64>() { directive = Some(seconds.saturating_mul(1_000)); }

                if directive.is_none() && let Some(seconds) = lower.strip_prefix("max-age=") && let Ok(seconds) = seconds.trim().parse::<u64>() { directive = Some(seconds.saturating_mul(1_000)); }

            }

        }

        let ttl = directive
            .or_else(|| headers.get(EXPIRES).filter(|_| !self.ignore.expires).and_then(Header::date).map(|expires| expires.saturating_mul(1_000).saturating_sub(now_ms)))
            .or_else(|| self.valid.iter().find(|( code, _ )| *code == status).map(|( _, ttl )| *ttl))
            .or(self.any)?;

        (ttl > 0).then_some(ttl)

    }

    pub fn claim ( self: &Arc<Self>, key: &Bytes ) -> Result<Claim, Arc<Semaphore>> {

        let gate = Arc::new(Semaphore::new(0));
        let fills = self.fills.pin();
        let held = fills.get_or_insert(key.clone(), gate.clone());

        match Arc::ptr_eq(held, &gate) {
            true => Ok(Claim { store: self.clone(), key: key.clone(), gate, stored: false }),
            false => Err(held.clone()),
        }

    }

    pub async fn wait ( &self, gate: Arc<Semaphore> ) {

        let _ = Rt::timeout("cache lock", self.lock_ms.max(1), gate.acquire()).await;

    }

    pub fn admit ( &self, key: &Bytes, headers: &HeaderMap ) -> bool {

        if self.ignore.vary || !headers.contains_key(VARY) { return true; }

        let Some(names) = Self::varies(headers) else { return false; };

        if names.is_empty() { return true; }

        let primary = key.slice(..key.iter().position(|byte| *byte == 0).unwrap_or(key.len()));

        if primary.len() < key.len() && let Some(marker) = self.entries.get(&primary) && *marker.vary == *names && key.starts_with(&Self::stem(&primary, marker.epoch)) { return true; }

        let marker = Arc::new(Stored { status: 0, headers: HeaderMap::new(), body: Bytes::new(), created_ms: 0, expires_ms: u64::MAX, refreshing: AtomicU64::new(0), vary: names.into(), epoch: Clock::now_ms().max(self.epoch.fetch_add(1, Ordering::Relaxed)) });

        self.shelve(&primary, &marker);
        self.entries.insert(primary, marker);

        false

    }

    fn varies ( headers: &HeaderMap ) -> Option<Vec<HeaderName>> {

        let mut names = Vec::new();

        for value in headers.get_all(VARY) {

            for token in value.to_str().ok()?.split(',').map(str::trim).filter(|token| !token.is_empty()) {

                if token == "*" { return None; }

                names.push(HeaderName::from_bytes(token.as_bytes()).ok()?);

            }

        }

        names.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        names.dedup();

        Some(names)

    }

    fn stem ( primary: &[u8], epoch: u64 ) -> BytesMut {

        let mut key = BytesMut::with_capacity(primary.len() + 96);

        key.extend_from_slice(primary);
        key.extend_from_slice(b"\0");
        key.extend_from_slice(epoch.to_string().as_bytes());
        key.extend_from_slice(b"\n");

        key

    }

    fn variant ( primary: &[u8], marker: &Stored, headers: &HeaderMap ) -> Bytes {

        let mut key = Self::stem(primary, marker.epoch);

        for name in &marker.vary {

            for value in headers.get_all(name) { key.extend_from_slice(value.as_bytes()); key.extend_from_slice(b","); }

            key.extend_from_slice(b"\n");

        }

        key.freeze()

    }

    pub fn fill ( self: &Arc<Self>, key: Bytes, status: u16, headers: &HeaderMap, ttl_ms: u64, tap: Probe, now_ms: u64 ) -> Fill {

        let wanted = |name: &HeaderName| !(Header::is_hop(name) || *name == TRANSFER_ENCODING || *name == CONTENT_LENGTH || *name == X_CACHE || *name == X_ACCEL_EXPIRES || *name == AGE || *name == SET_COOKIE);
        let mut block = BytesMut::with_capacity(headers.iter().filter(|( name, _ )| wanted(name)).map(|( _, value )| value.len()).sum());

        for ( _, value ) in headers.iter().filter(|( name, _ )| wanted(name)) { block.extend_from_slice(value.as_bytes()); }

        let block = block.freeze();
        let mut kept = HeaderMap::with_capacity(headers.len());
        let mut offset = 0;

        for ( name, value ) in headers.iter().filter(|( name, _ )| wanted(name)) {

            let end = offset + value.len();

            if let Ok(owned) = HeaderValue::from_maybe_shared(block.slice(offset..end)) { kept.append(name, owned); }

            offset = end;

        }

        Fill { store: self.clone(), key, status, headers: kept, ttl_ms, created_ms: now_ms, tap, claim: None }

    }

    pub fn respond ( entry: &Stored, method: &Method, headers: &HeaderMap, now_ms: u64, stale: bool ) -> Res {

        if Header::fresh(headers, entry.headers.get(ETAG), entry.headers.get(LAST_MODIFIED).and_then(Header::date)) {

            let mut response = Response::status(304);

            for name in [ETAG, LAST_MODIFIED, CACHE_CONTROL, EXPIRES, VARY] { if let Some(value) = entry.headers.get(&name) { response.headers_mut().insert(name, value.clone()); } }

            response.headers_mut().remove(CONTENT_LENGTH);
            response.headers_mut().insert(X_CACHE, HeaderValue::from_static("HIT"));

            return response;

        }

        let part = Self::part(entry, method, headers);
        let mut response = Response::status(match part { Some(Ok(_)) => 206, Some(Err(())) => 416, None => entry.status });

        *response.headers_mut() = entry.headers.clone();

        match part {
            Some(Ok(( start, end ))) => {

                response.headers_mut().insert(CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {start}-{end}/{}", entry.body.len())).unwrap_or(HeaderValue::from_static("bytes */*")));
                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(end - start + 1));
                response.headers_mut().insert(X_CACHE, HeaderValue::from_static("HIT"));

                *response.body_mut() = Body::bytes(entry.body.slice(start..=end));

                return response;

            }
            Some(Err(())) => {

                response.headers_mut().insert(CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{}", entry.body.len())).unwrap_or(HeaderValue::from_static("bytes */*")));
                response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(0usize));

                return response;

            }
            None => {}
        }

        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(entry.body.len()));
        response.headers_mut().insert(AGE, HeaderValue::from(now_ms.saturating_sub(entry.created_ms) / 1_000));
        response.headers_mut().insert(X_CACHE, HeaderValue::from_static(if stale { "STALE" } else { "HIT" }));

        if method != Method::HEAD && !entry.body.is_empty() { *response.body_mut() = Body::bytes(entry.body.clone()); }

        response

    }

    fn part ( entry: &Stored, method: &Method, headers: &HeaderMap ) -> Option<Result<( usize, usize ), ()>> {

        let spec = headers.get(RANGE).filter(|_| method == Method::GET && entry.status == 200 && !entry.body.is_empty())?.to_str().ok()?;

        if let Some(condition) = headers.get(IF_RANGE) && entry.headers.get(ETAG) != Some(condition) && entry.headers.get(LAST_MODIFIED) != Some(condition) { return None; }

        let parsed = http_range_header::parse_range_header(spec.trim()).ok().filter(|parsed| parsed.ranges.len() == 1)?;

        Some(match parsed.validate(entry.body.len() as u64) {
            Ok(ranges) => ranges.first().map(|range| ( *range.start() as usize, *range.end() as usize )).ok_or(()),
            Err(http_range_header::RangeUnsatisfiableError::FileSuffixOutOfBounds) => Ok(( 0, entry.body.len() - 1 )),
            Err(_) => Err(()),
        })

    }

    pub fn purge ( &self, key: &Bytes ) -> bool {

        let shelved = self.shelf.as_ref().is_some_and(|shelf| shelf.remove(key));

        self.entries.remove(key) || shelved

    }

    pub fn clear ( &self ) {

        self.entries.clear();

        if let Some(shelf) = &self.shelf { shelf.clear(); }

        if let Ok(mut bans) = self.bans.write() { bans.clear(); self.banned.store(0, Ordering::Relaxed); }

    }

    pub fn ban ( &self, ban: Ban ) -> usize {

        let full = self.bans.read().is_ok_and(|bans| bans.len() >= BANS_MAX);

        if full { self.clear(); return 0; }

        let Ok(mut bans) = self.bans.write() else { return 0; };

        bans.push(ban);
        self.banned.store(bans.len() as u64, Ordering::Relaxed);

        bans.len()

    }

    fn barred ( &self, key: &[u8], entry: &Stored ) -> bool {

        let Ok(bans) = self.bans.read() else { return false; };
        let split = key.iter().position(|byte| *byte == b' ').unwrap_or(key.len());
        let ( host, target ) = ( &key[..split], key.get(split + 1..).unwrap_or_default() );

        bans.iter().any(|ban| entry.created_ms <= ban.at_ms
            && ban.host.as_deref().is_none_or(|wanted| host.eq_ignore_ascii_case(wanted.as_bytes()))
            && ban.prefix.as_deref().is_none_or(|prefix| target.starts_with(prefix.as_bytes()))
            && ban.tag.as_deref().is_none_or(|tag| [CACHE_TAG, SURROGATE_KEY].iter().flat_map(|name| entry.headers.get_all(name)).filter_map(|value| value.to_str().ok()).flat_map(|value| value.split([',', ' '])).any(|held| held == tag)))

    }

    pub fn shelved ( &self, key: &Bytes, now_ms: u64 ) -> bool {

        self.shelf.as_ref().is_some_and(|shelf| shelf.holds(key, now_ms))

    }

    pub async fn thaw ( &self, key: Bytes, now_ms: u64 ) -> bool {

        let Some(shelf) = self.shelf.clone() else { return false; };
        let wanted = key.clone();
        let Ok(Some(( expires_ms, payload ))) = tokio::task::spawn_blocking(move || shelf.get(&wanted, now_ms)).await else { return false; };
        let Some(entry) = Stored::decode(&payload, expires_ms) else { return false; };

        self.entries.insert(key, Arc::new(entry))

    }

    fn shelve ( &self, key: &Bytes, entry: &Stored ) {

        let Some(shelf) = self.shelf.clone() else { return; };
        let ( key, payload, expires_ms ) = ( key.clone(), entry.encode(), entry.expires_ms );

        if tokio::runtime::Handle::try_current().is_ok() { tokio::task::spawn_blocking(move || shelf.put(&key, expires_ms, &payload)); }

    }

    pub fn status_code ( code: u16 ) -> StatusCode {

        StatusCode::from_u16(code).unwrap_or(StatusCode::OK)

    }

    pub fn describe ( &self ) -> Value {

        json!({
            "entries"  : self.entries.len(),
            "bytes"    : self.entries.weight(),
            "capacity" : self.entries.capacity(),
            "hits"     : self.hits.load(Ordering::Relaxed),
            "misses"   : self.misses.load(Ordering::Relaxed),
            "stale"    : self.stale.load(Ordering::Relaxed),
            "filled"   : self.filled.load(Ordering::Relaxed),
            "bans"     : self.banned.load(Ordering::Relaxed),
        })

    }

}

impl Stored {

    fn encode ( &self ) -> Vec<u8> {

        let vary = self.vary.iter().map(HeaderName::as_str).collect::<Vec<_>>().join(",");
        let mut head = Vec::with_capacity(512);

        for ( name, value ) in &self.headers {

            head.extend_from_slice(name.as_str().as_bytes());
            head.extend_from_slice(b": ");
            head.extend_from_slice(value.as_bytes());
            head.extend_from_slice(b"\r\n");

        }

        let mut out = Vec::with_capacity(32 + vary.len() + head.len() + self.body.len());

        out.extend_from_slice(&self.status.to_le_bytes());
        out.extend_from_slice(&self.created_ms.to_le_bytes());
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(&(vary.len() as u16).to_le_bytes());
        out.extend_from_slice(vary.as_bytes());
        out.extend_from_slice(&(head.len() as u32).to_le_bytes());
        out.extend_from_slice(&head);
        out.extend_from_slice(&self.body);

        out

    }

    fn decode ( bytes: &[u8], expires_ms: u64 ) -> Option<Self> {

        let status = u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?);
        let created_ms = u64::from_le_bytes(bytes.get(2..10)?.try_into().ok()?);
        let epoch = u64::from_le_bytes(bytes.get(10..18)?.try_into().ok()?);
        let named = usize::from(u16::from_le_bytes(bytes.get(18..20)?.try_into().ok()?));
        let vary = std::str::from_utf8(bytes.get(20..20 + named)?).ok()?.split(',').filter(|name| !name.is_empty()).map(|name| HeaderName::from_bytes(name.as_bytes()).ok()).collect::<Option<Vec<_>>>()?;
        let at = 20 + named;
        let length = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        let head = bytes.get(at + 4..at + 4 + length)?;
        let mut headers = HeaderMap::new();

        for line in head.split(|byte| *byte == b'\n').map(|line| line.strip_suffix(b"\r").unwrap_or(line)).filter(|line| !line.is_empty()) {

            let split = line.iter().position(|byte| *byte == b':')?;

            headers.append(HeaderName::from_bytes(&line[..split]).ok()?, HeaderValue::from_bytes(line.get(split + 2..)?).ok()?);

        }

        Some(Self { status, headers, body: Bytes::copy_from_slice(&bytes[at + 4 + length..]), created_ms, expires_ms, refreshing: AtomicU64::new(0), vary: vary.into(), epoch })

    }

}

impl Fill {

    pub fn claimed ( self, claim: Option<Claim> ) -> Self {

        let mut fill = self;

        fill.claim = claim;

        fill

    }

}

impl Drop for Fill {

    fn drop ( &mut self ) {

        let tap = self.tap.borrow();

        let Some(whole) = tap.whole() else { return; };

        if whole.len() > self.store.max_object { return; }

        let created_ms = Clock::now_ms().max(self.created_ms);
        let entry = Arc::new(Stored {
            status     : self.status,
            headers    : std::mem::take(&mut self.headers),
            body       : Bytes::copy_from_slice(whole),
            created_ms,
            expires_ms : created_ms.saturating_add(self.ttl_ms),
            refreshing : Default::default(),
            vary       : Box::default(),
            epoch      : 0,
        });

        self.store.shelve(&self.key, &entry);

        if self.store.entries.insert(self.key.clone(), entry) { self.store.filled.fetch_add(1, Ordering::Relaxed); }

        if let Some(claim) = &mut self.claim { claim.stored = true; }

    }

}

impl Drop for Claim {

    fn drop ( &mut self ) {

        if !self.stored && self.store.entries.get(&self.key).is_none() {

            self.store.entries.insert(self.key.clone(), Arc::new(Stored { status: 0, headers: HeaderMap::new(), body: Bytes::new(), created_ms: 0, expires_ms: Clock::now_ms().saturating_add(CACHE_PASS_MS), refreshing: AtomicU64::new(0), vary: Box::default(), epoch: 0 }));

        }

        self.store.fills.pin().remove(&self.key);
        self.gate.close();

    }

}
