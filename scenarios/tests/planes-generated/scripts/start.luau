-- The run begins on the Ethereal Plane.
local start = {}

function start.shift(ctx, facts)
    for _, actor in ipairs(ctx:characters()) do
        ctx:shift(actor, "ethereal")
    end
end

return start
