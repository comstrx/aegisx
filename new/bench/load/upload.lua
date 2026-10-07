local tokens = dofile("load/tokens.lua")
local body = string.rep("u", 262144)

function request ()

    local who = tokens[math.random(#tokens)]

    return wrk.format("POST", "/api/files", { ["X-Tenant"] = who.tenant, ["Authorization"] = "Bearer " .. who.token, ["Content-Type"] = "application/octet-stream" }, body)

end
