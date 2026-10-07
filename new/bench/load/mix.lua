local tokens = dofile("load/tokens.lua")
local threads = 0

function setup ( thread )

    thread:set("seed", threads)
    threads = threads + 1

end

function init ()

    math.randomseed(os.time() + seed * 7919)

end

local function json ( who, method, path, body )

    return wrk.format(method, path, { ["X-Tenant"] = who.tenant, ["Authorization"] = "Bearer " .. who.token, ["Content-Type"] = "application/json", ["Accept"] = "application/json" }, body)

end

function request ()

    local who = tokens[math.random(#tokens)]
    local tenant = tonumber(who.tenant:sub(2))
    local item = (math.random(320) - 1) * 32 + (math.random(8) - 1) * 4 + tenant
    local roll = math.random(100)

    if roll <= 55 then

        local filter = math.random(3) == 1 and ("&category=" .. ((math.random(8) - 1) * 4 + tenant)) or ""

        return wrk.format("GET", "/api/catalog?page=" .. math.random(40) .. "&size=20" .. filter, { ["X-Tenant"] = who.tenant, ["Accept"] = "application/json" })

    elseif roll <= 80 then

        return wrk.format("GET", "/api/catalog/" .. item, { ["X-Tenant"] = who.tenant, ["Accept"] = "application/json" })

    elseif roll <= 88 then

        return json(who, "GET", "/api/me/favorites")

    elseif roll <= 92 then

        return json(who, "PUT", "/api/me/favorites/" .. item)

    elseif roll <= 96 then

        return json(who, "POST", "/api/orders", '{"lines":[{"item":' .. item .. ',"quantity":1}]}')

    elseif roll <= 98 then

        return json(who, "POST", "/api/catalog/" .. item .. "/comments", '{"body":"measured under load"}')

    end

    return wrk.format("POST", "/api/auth/token", { ["X-Tenant"] = who.tenant, ["Content-Type"] = "application/json" }, '{"email":"user1@' .. who.tenant .. '.test","password":"secret"}')

end
