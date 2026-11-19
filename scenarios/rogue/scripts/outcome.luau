-- Winning: the hero climbs out of the dungeon carrying the Amulet of Yendor.
-- Losing: the hero dies. Climbing out without the amulet changes nothing.
local outcome = {}

local AMULET = "item.amulet.amulet-of-yendor"
local EXIT = "dungeon/1/up"

local function carries_amulet(ctx, actor)
    for _, item in ipairs(ctx:carried(actor)) do
        if ctx:item_identity(item) == AMULET then
            return true
        end
    end
    return false
end

function outcome.start(ctx, facts)
    ctx:set_goal(
        "Find the Amulet of Yendor deep in the dungeon and climb back out with it.",
        { cell = ctx:anchor(EXIT), to = ctx:selected() }
    )
end

function outcome.escaped(ctx, facts)
    local actor = facts[1].actor
    if carries_amulet(ctx, actor) then
        ctx:win(actor)
    else
        ctx:message("You need the Amulet of Yendor to leave the dungeon.", { style = "hint", to = actor })
    end
end

function outcome.died(ctx, facts)
    ctx:lose(facts[1].actor)
end

return outcome
