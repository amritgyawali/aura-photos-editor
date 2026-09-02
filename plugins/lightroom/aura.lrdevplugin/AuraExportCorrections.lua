--[[
  Export a photographer's corrections back to AURA as learning-loop input.

  ## What counts as a correction here, and what does not

  Only a difference from **what this plugin imported**. The import stamps each photograph's
  develop settings into a plugin-private field; the export compares against that stamp.

  A slider the photographer had already moved before the import is not reported, and neither is a
  photograph AURA never graded. That is the same distinction `AURA-LRN-11004` makes on the other
  side of the boundary: a change with no decision behind it is not a correction of anything, and a
  residual measured from no baseline is an absolute edit wearing a residual's shape.

  Without this rule, a photographer who imported AURA's grading into a catalog they had already
  worked would export their own eight-year-old style as four thousand corrections of AURA.

  ## The file this writes

  `aura-corrections.json`, which `aura-app`'s learning panel reads. One row per photograph per
  changed parameter, carrying the decision id the import recorded - which is what makes the
  attribution possible at all.
--]]

local LrApplication = import "LrApplication"
local LrDialogs = import "LrDialogs"
local LrFileUtils = import "LrFileUtils"
local LrPathUtils = import "LrPathUtils"
local LrTasks = import "LrTasks"

local Json = require "AuraJson"

-- The develop keys that map onto `Learnable`. Ten of the fifteen; the other five are thresholds
-- rather than develop settings and are corrected in AURA's own panels.
--
-- Nothing that names a guarantee is here, and nothing can be added: `Learnable` is closed on the
-- Rust side and a key this table invented would be refused by `AURA-LRN-11002`.
local LEARNABLE = {
    Exposure2012 = "exposure",
    Temperature = "temperature_k",
    Tint = "tint",
    Contrast2012 = "contrast",
    Highlights2012 = "highlights",
    Shadows2012 = "shadows",
    Whites2012 = "whites",
    Blacks2012 = "blacks",
    Vibrance = "vibrance",
    Saturation = "saturation",
}

--- How far a value has to move before it is worth reporting.
---
--- A photographer opening a panel and closing it must not produce four thousand rows of nothing;
--- the Rust side drops them anyway (`Correction::is_material`) and doing it here saves the file
--- being the size of the catalog.
local MATERIAL = {
    exposure = 0.01,
    temperature_k = 20.0,
    tint = 0.5,
    contrast = 1.0,
    highlights = 1.0,
    shadows = 1.0,
    whites = 1.0,
    blacks = 1.0,
    vibrance = 1.0,
    saturation = 1.0,
}

local function corrections(catalog)
    local rows = {}
    for _, photo in ipairs(catalog:getTargetPhotos()) do
        -- What the import stamped. No stamp means AURA never graded this frame, so nothing here
        -- is a correction *of* anything.
        local stamped = photo:getPropertyForPlugin(_PLUGIN, "auraBaseline")
        local decision = photo:getPropertyForPlugin(_PLUGIN, "auraDecisionId")
        if stamped and decision then
            local ok, baseline = pcall(Json.decode, stamped)
            if ok and type(baseline) == "table" then
                local now = photo:getDevelopSettings()
                for key, learnable in pairs(LEARNABLE) do
                    local was = tonumber(baseline[key])
                    local is = tonumber(now[key])
                    if was and is then
                        local delta = is - was
                        if math.abs(delta) >= (MATERIAL[learnable] or 0) then
                            rows[#rows + 1] = {
                                decision_id = decision,
                                learnable = learnable,
                                before = was,
                                after = is,
                                magnitude = delta,
                                photo = photo:getRawMetadata("path"),
                            }
                        end
                    end
                end
            end
        end
    end
    return rows
end

LrTasks.startAsyncTask(function()
    local catalog = LrApplication.activeCatalog()
    local rows = corrections(catalog)

    if #rows == 0 then
        LrDialogs.message(
            "Nothing to send back",
            "None of the selected photographs has a change AURA can learn from. AURA only learns "
                .. "from changes to photographs it graded itself.",
            "info"
        )
        return
    end

    local folder = LrDialogs.runOpenPanel({
        title = "Where should the corrections go?",
        canChooseFiles = false,
        canChooseDirectories = true,
        allowsMultipleSelection = false,
    })
    if not folder or #folder == 0 then
        return
    end

    local path = LrPathUtils.child(folder[1], "aura-corrections.json")
    local document = Json.encode({
        schema = "aura.corrections/1",
        source = "lightroom-classic",
        corrections = rows,
    })
    LrFileUtils.writeFile(path, document)

    LrDialogs.message(
        "Corrections written",
        #rows .. " changes saved to aura-corrections.json. Open the Learning panel in AURA to "
            .. "review what it would do with them - nothing changes your profile until you say so.",
        "info"
    )
end)
