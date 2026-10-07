# AegisX transport patch to Pingora Core 0.9.0

Original Apache-2.0 source is retained; original file hashes are in AEGISX-PATCH.json.
Runtime changes are limited to src/protocols/http/v1/server.rs.
The v1/mod.rs test mock also supports controlled pending flushes for cancellation regression coverage.

Flush the downstream buffered writer after each already-ready HTTP task batch.
This permits coalescing headers and bodies already available together without waiting
for the next chunk. Header-only and non-final body batches also flush. The cancel-safe
API retains flush_pending across cancellation and respects the configured write timeout.
AegisX uses a bounded 4 KiB L4 write buffer; zero remains supported through Lua.

No framing/parser, retry, authorization, TLS or body-size checks are removed.
Retain this patch only if transport regression tests and paired benchmarks support it.
The crate is a vendored dependency of the single application crate, not a second server.
