-- Wait for transient resource pressure before forwarding. Zero capacity disables waiting.
set_queue { capacity=256, timeout_ms=1000 }

-- One existing application, one executable, one configuration.
set_listen("127.0.0.1:8080")
set_upstream("127.0.0.1:3000")

set_balancer("default", {
    policy = "adaptive",
    max_fails = 2,
    cooldown_ms = 5000,
    health_interval_ms = 1000,
    health_timeout_ms = 300,
    connect_attempts = 1
})

-- Research text/journey model; independent application validation is required.
-- All inference happens after forwarding completes.
set_model {
    mode = "observe",
    threshold = 0.95,
    queue_capacity = 64,
    on_overload = "reject",
    scan_bytes = 16384,
    -- Explicit opt-in: bounded plaintext response sample stays in volatile memory.
    response_scan_bytes = 1024,
    journey = true,
    max_queue_age_ms = 5000
}
set_store("aegisx.sqlite3")
set_persistence { admission = "required", synchronous = "full" }

set_limits {
    timeout_ms = 10000,
    max_body_bytes = 1048576,
    max_sources = 10000,
    queue_capacity = 1024,
    retention_events = 100000,
    rate_limit_10s = 0
}
set_runtime {
    threads = 2,
    work_stealing = false,
    accept_tasks = 1,
    upstream_keepalive_capacity = 128,
    max_in_flight = 4096,
    write_buffer_bytes = 0,
    keepalive_seconds = 30,
    pool_idle_seconds = 30
}

add_route {
    name = "health",
    path = "/health",
    exact = true,
    model = "off",
    capture = false
}
add_route {
    name = "api",
    path = "/api",
    rate_limit_10s = 100
}

-- Optional TLS termination:
-- set_tls { cert = "certificates/fullchain.pem", key = "certificates/key.pem" }
-- Add explicit backend groups with add_upstream and set_default_upstream.
-- See README.md for multi-backend, verified upstream TLS and per-route policies.

-- Independent features; enable only what the deployment needs.
set_context { shards = 16, idle_ttl_ms = 120000 }
set_cache { decisions = false, scores = false, responses = false }
set_telemetry { enabled = true, capture = "full", sample_every = 1, recent_capacity = 256 }
set_identity { request_id_header = "x-request-id", propagate = true, forwarding = true }
set_control { enabled = false, listen = "127.0.0.1:9090", token_env = "AEGISX_ADMIN_TOKEN" }
set_webhooks { enabled = false }
-- See configuration.md and contracts/README.md for complete integration controls.
