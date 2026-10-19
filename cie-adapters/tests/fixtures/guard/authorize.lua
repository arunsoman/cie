local Authorize = {}

function Authorize.authorize(user)
    return user ~= nil
end

function Authorize.run(u)
    Authorize.authorize(u)
    return true
end

return Authorize
