
use crate::config::base::consts::DEFAULT_POOL;
use crate::core::env::Env;
use crate::core::error::{AppError, AppResult};
use http::header::HeaderValue;

use crate::core::net::Address;
use crate::http::encode::Compression;
use crate::http::files::FileCache;
use crate::http::h3::QuicSettings;
use crate::http::upstream::Settings as ClientSettings;
use crate::http::server::Settings as ServerSettings;
use super::arch::{BackendConfig, Config, PoolConfig};

impl Config {

    pub fn worker_count ( &self ) -> usize {

        match self.runtime.workers {
            0 => Env::cores(),
            count => count,
        }

    }

    pub fn server_settings ( &self, proxy_protocol: bool ) -> ServerSettings {

        ServerSettings {
            keepalive         : self.server.keepalive,
            max_headers       : self.server.max_headers,
            max_connections   : self.server.max_connections.div_ceil(self.worker_count().max(1)),
            header_timeout_ms : self.server.header_timeout_ms,
            keepalive_timeout_ms : self.server.keepalive_timeout_ms,
            send_timeout_ms   : self.server.send_timeout_ms,
            buffer            : self.server.buffer,
            drain_ms          : self.server.drain_ms,
            http2             : self.server.http2,
            h2c               : self.server.h2c,
            max_streams       : self.server.max_streams,
            h2_adaptive_window: self.server.h2_adaptive_window,
            h2_stream_window  : self.server.h2_stream_window,
            h2_connection_window : self.server.h2_connection_window,
            h2_max_frame      : self.server.h2_max_frame,
            h2_max_header_bytes : self.server.h2_max_header_bytes,
            proxy_protocol,
        }

    }

    pub fn quic_settings ( &self ) -> Option<QuicSettings> {

        self.http3.enabled.then(|| QuicSettings { listen: self.http3.listen.unwrap_or(self.listen), max_idle_ms: self.http3.max_idle_ms, max_streams: self.http3.max_streams, shared: cfg!(target_os = "linux"), congestion: self.http3.congestion, stream_window: self.http3.stream_window, send_window: self.http3.send_window })

    }

    pub fn alt_svc ( &self ) -> Option<HeaderValue> {

        self.http3.enabled.then(|| HeaderValue::from_str(&format!("h3=\":{}\"; ma={}", self.http3.listen.unwrap_or(self.listen).port(), self.http3.alt_svc_max_age)).ok()).flatten()

    }

    pub fn file_cache ( &self ) -> FileCache {

        FileCache::new(self.files.cache_items, self.files.cache_bytes, self.files.max_file_bytes, self.files.valid_ms, self.files.precompressed)

    }

    pub fn compression ( &self ) -> Compression {

        Compression::new(self.compression.min_bytes, self.compression.level, self.compression.brotli, self.compression.brotli_level, self.compression.zstd, self.compression.zstd_level, self.compression.types.iter().cloned())

    }

    pub fn client_settings ( &self ) -> ClientSettings {

        ClientSettings {
            connect_timeout_ms : self.client.connect_timeout_ms,
            pool_idle_ms       : self.client.pool_idle_ms,
            pool_capacity      : self.client.pool_capacity,
            buffer             : self.client.buffer,
            attempts           : self.client.attempts,
        }

    }

    pub fn set_upstream ( &mut self, address: impl Into<Address> ) {

        let address = address.into();

        let pool = self.pools.entry(DEFAULT_POOL.to_string()).or_default();

        pool.backends = vec![BackendConfig { address, ..BackendConfig::default() }];

        if self.default_pool.is_none() { self.default_pool = Some(DEFAULT_POOL.to_string()); }

    }

    pub fn default_pool ( &self ) -> AppResult<( &str, &PoolConfig )> {

        let name = self.default_pool.as_deref().ok_or_else(|| AppError::config("default_upstream", "no default upstream pool is configured"))?;
        let pool = self.pools.get(name).ok_or_else(|| AppError::config("default_upstream", format!("pool `{name}` is not defined")))?;

        Ok(( name, pool ))

    }

    pub fn primary_upstream ( &self ) -> AppResult<Address> {

        let ( name, pool ) = self.default_pool()?;

        pool.backends.first().map(|backend| backend.address.clone()).ok_or_else(|| AppError::config("upstream", format!("pool `{name}` has no backends")))

    }

}
