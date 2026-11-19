-- Winning (play continues): a character stands on the companion cell
-- carrying the dawn seal. The goal is not disclosed.
-- Losing: the selected character dies.
local outcome = {}

local PLACE = "1/companion"
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
    if ctx:alive(actor) and stands_on(ctx, actor, ctx:anchor(PLACE)) and carries(ctx, actor, SEAL) then
        ctx:win(actor, { continue = true })
        return true
    end
    return false
end

-- A companion may start on the cell already carrying the seal.
function outcome.start(ctx, facts)
    for _, actor in ipairs(ctx:characters()) do
        if try_win(ctx, actor) then
            return
        end
    end
end

-- Arriving at the cell, or taking the seal while standing on it.
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
