set_identity { propagate = false, forwarding = false }
set_telemetry { enabled = false }
set_client { attempts = 1 }
set_upstream("10.42.1.170:3900")
set_balancer("default", { attempts = 1 })
