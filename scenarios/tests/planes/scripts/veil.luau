-- Waiting at the altar shifts the waiter to the Ethereal Plane.
local veil = {}

function veil.altar(ctx, facts)
    local actor = facts[1].actor
    if ctx:plane(actor) == "material" then
        ctx:shift(actor, "ethereal")
    end
end

function veil.shifted(ctx, facts)
    local fact = facts[1]
    if fact.plane == "material" then
        return
    end
    ctx:message("The world greys and thins around you.", { style = "narration", to = fact.actor })
    -- One Lore check per arrival on a plane the traveller does not know.
    if not ctx:knows_plane(fact.actor, fact.plane) and ctx:check(fact.actor, "lore", 14) then
        ctx:reveal_plane(fact.actor, fact.plane)
    end
end

function veil.blocked(ctx, facts)
    ctx:message("Something solid holds you here.", { style = "narration", to = facts[1].actor })
end

return veil
