-- The haunted chapel's narration, the ghosts' glimpses and arrival Lore.
local haunting = {}

function haunting.cold_air(ctx, facts)
    ctx:message("The chapel is colder than the night outside.",
        { style = "narration", to = facts[1].actor })
end

-- A ghost's touch: the victim may glimpse it until the ghost acts again.
function haunting.struck(ctx, facts)
    for _, fact in ipairs(facts) do
        if ctx:is_character(fact.target) then
            if ctx:check(fact.target, "perception", 15) then
                ctx:open_channel(fact.target, fact.actor)
                ctx:message("For a heartbeat you see it: a pale shape in the air beside you.",
                    { style = "narration", to = fact.target })
            else
                ctx:message("Something unseen and cold passes through you.",
                    { style = "narration", to = fact.target })
            end
        end
    end
end

-- One Lore check per arrival on a plane the traveller does not know.
function haunting.arrived(ctx, facts)
    for _, fact in ipairs(facts) do
        if fact.plane ~= "material" and not ctx:knows_plane(fact.actor, fact.plane)
            and ctx:check(fact.actor, "lore", 14) then
            ctx:reveal_plane(fact.actor, fact.plane)
        end
    end
end

return haunting
