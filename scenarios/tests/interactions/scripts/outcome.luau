-- Losing: the run ends when the selected character dies. There is no win.
local outcome = {}

function outcome.died(ctx, facts)
    ctx:lose(facts[1].actor)
end

return outcome
