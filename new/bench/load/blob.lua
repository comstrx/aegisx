function request ()

    return wrk.format("GET", "/api/blob?bytes=1048576", { ["X-Tenant"] = "t1" })

end
