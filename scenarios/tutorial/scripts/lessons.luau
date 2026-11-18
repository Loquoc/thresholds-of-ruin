-- Lessons for a first walk. Each handler runs when a hook in scenario.toml
-- hears its fact; state that must last goes through variables.
local lessons = {}

local function advance(ctx)
    ctx:set_var("lessons", ctx:var("lessons") + 1)
end

function lessons.welcome(ctx, facts)
    ctx:message("Welcome to the practice hall. Walk east with {control:move}.")
    ctx:message("Whenever you are unsure what to do, {control:help} lists your commands.", { style = "hint" })
    ctx:start_timer("lessons.nudge", 1000)
end

-- Ten turns in, a reminder for anyone who has not moved yet.
function lessons.nudge(ctx, facts)
    if ctx:var("lessons") == 0 then
        ctx:message("Try {control:move} toward the east. You can also {control:wait} to let time pass.", { style = "hint" })
    end
end

function lessons.at_token(ctx, facts)
    advance(ctx)
    ctx:message("Something small lies here. Pick it up with {control:pick_up}.", { to = facts[1].actor })
end

function lessons.took_token(ctx, facts)
    advance(ctx)
    ctx:set_var("has_token", true)
    ctx:message("Your {control:inventory} shows everything you carry.", { to = facts[1].actor })
end

function lessons.at_door(ctx, facts)
    advance(ctx)
    ctx:message("A closed door blocks the way. Open it with {control:open_door}.", { to = facts[1].actor })
end

function lessons.door_changed(ctx, facts)
    advance(ctx)
    ctx:message("Doors stay as you leave them. The way out is at the far end of the hall.", { style = "narration" })
end

function lessons.exit(ctx, facts)
    local actor = facts[1].actor
    if ctx:var("has_token") then
        ctx:message("You found the token and the way out. That is the whole lesson.", { to = actor })
        ctx:win(actor)
    else
        ctx:message("Go back for the token before you leave.", { style = "hint", to = actor })
    end
end

return lessons
