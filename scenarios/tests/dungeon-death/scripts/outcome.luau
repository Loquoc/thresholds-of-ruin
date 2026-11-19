-- Winning: a character stands on the exit carrying the dawn seal.
-- Losing: the selected character dies.
local outcome = {}

local EXIT = "1/exit"
local SEAL = "item:100"

local function carries(ctx, actor, item)
    for _, carried in ipairs(ctx:carried(actor)) do
        if carried == item then
            return true
        end
    end
    return false
end

local function stands_on(ctx, actor, cell)
    local here = ctx:location(actor)
    return here ~= nil
        and here.region == cell.region
        and here.x == cell.x
        and here.y == cell.y
        and here.z == cell.z
end

local function try_win(ctx, actor)
    if ctx:alive(actor) and stands_on(ctx, actor, ctx:anchor(EXIT)) and carries(ctx, actor, SEAL) then
        ctx:win(actor)
        return true
    end
    return false
end

-- A character may already start on the exit with the seal.
function outcome.start(ctx, facts)
    ctx:set_goal("Bring the dawn seal back to the exit.", { cell = ctx:anchor(EXIT) })
    for _, actor in ipairs(ctx:characters()) do
        if try_win(ctx, actor) then
            return
        end
    end
end

-- Arriving at the exit, or taking the seal while standing on it.
function outcome.check(ctx, facts)
    for _, fact in ipairs(facts) do
        if try_win(ctx, fact.actor) then
            return
        end
    end
end

function outcome.died(ctx, facts)
    ctx:lose(facts[1].actor)
end

return outcome
