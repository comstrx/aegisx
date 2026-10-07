# AegisX timeout allocation patch to Pingora Timeout 0.9.0

Original source hashes are in AEGISX-PATCH.json. Apache-2.0 license retained.
ToTimeout::timeout already returns Pin<Box<dyn Future<Output=()> + Send + Sync>>.
Store that pinned future directly instead of allocating another Box around it.
Lazy initialization, deadline handling, cancellation, and the public API are unchanged.
