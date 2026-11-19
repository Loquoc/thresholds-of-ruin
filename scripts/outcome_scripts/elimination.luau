-- An arena ends when at most one faction still has a living participant.
-- The participants are the arena's characters; the last faction standing
-- wins, and if everyone fell nobody does.
local outcome = {}

function outcome.died(ctx, facts)
    local factions, living = 0, 0
    local seen, standing, winner = {}, {}, nil
    for _, actor in ipairs(ctx:characters()) do
        local faction = ctx:faction(actor)
        if faction ~= nil then
            if not seen[faction] then
                seen[faction] = true
                factions += 1
            end
            if ctx:alive(actor) and not standing[faction] then
                standing[faction] = true
                living += 1
                winner = faction
            end
        end
    end
    if living == 0 then
        ctx:end_arena(nil)
    elseif factions > 1 and living == 1 then
        ctx:end_arena(winner)
    end
end

return outcome
