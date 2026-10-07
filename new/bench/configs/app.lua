set_upstream("127.0.0.1:3800")

set_server {
    keepalive_requests = 10000,
    keepalive_time_ms  = 3600000,
    send_timeout_ms    = 30000,
}

set_limits {
    timeout_ms      = 15000,
    max_body_bytes  = 67108864,
    rate_per_second = 20000,
    rate_burst      = 20000,
}

set_balancer("default", {
    policy   = "least_time",
    attempts = 2,
    retry_on = { "connect", "error" },
    health   = { path = "/ready", interval_ms = 2000, timeout_ms = 1000 },
})

set_identity { traceparent = true }

set_cache {
    enabled        = true,
    valid_ms       = { ["200"] = 5000 },
    stale_ms       = 30000,
    stale_if_error = true,
    key_headers    = { "x-tenant" },
}

set_compression { enabled = true }

set_headers("response", {
    ["-server-timing"]            = "",
    ["x-content-type-options"]    = "nosniff",
    ["?cache-control"]            = "no-store",
})

add_route { name = "health", path = "/healthz", exact = true, respond = { body = "ok" } }
add_route { name = "catalog", path = "/api/catalog", cache = true, methods = { "GET", "HEAD", "POST" } }
add_route { name = "login", path = "/api/auth", rate_per_second = 200, rate_burst = 400 }
add_route { name = "orders", path = "/api/orders", buffer_request = true }
add_route { name = "events", path = "/api/events", compress = false }
add_route { name = "all", path = "/" }
