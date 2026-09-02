--[[
  Import AURA's selections, labels and grading into a Lightroom catalog.

  ## The order matters and it is not obvious

  Selections first, then labels, then grading. A photographer who cancels half-way through has
  their picks and their labels, which is the half of the import that is expensive to redo by hand;
  the grading is the half a re-run reproduces exactly, because it comes from a sidecar rather than
  from a decision.

  ## What "graceful degradation" means here

  On Lightroom 12 the develop-settings API accepts a smaller set of keys than 13 and 14 do. Rather
  than writing what it can and leaving the rest silently unapplied - which is a frame that looks
  nearly right - this plugin detects the version, applies only the parameters that survive
  round-tripping on that release, and *says which ones it skipped*. Section 12's row on plugin
  breakage, and phase 24's rule about an absent capability: "not applied" and "applied at zero" must
  never render the same.
--]]

local LrApplication = import "LrApplication"
local LrDialogs = import "LrDialogs"
local LrFileUtils = import "LrFileUtils"
local LrPathUtils = import "LrPathUtils"
local LrTasks = import "LrTasks"
local LrFunctionContext = import "LrFunctionContext"

local Json = require "AuraJson"

-- The develop keys this plugin writes, and the lowest Lightroom Classic release each survives a
-- round trip on. A key absent from a release's set is skipped and named rather than written.
local DEVELOP_KEYS = {
    Exposure2012 = 12,
    Temperature = 12,
    Tint = 12,
    Contrast2012 = 12,
    Highlights2012 = 12,
    Shadows2012 = 12,
    Whites2012 = 12,
    Blacks2012 = 12,
    Vibrance = 12,
    Saturation = 12,
    -- The parametric tone curve round-trips reliably from 13 onward. On 12 it is skipped, which
    -- costs a photographer the curve and not the frame.
    ToneCurvePV2012 = 13,
}

local function majorVersion()
    local v = LrApplication.versionTable()
    return v and v.major or 0
end

--- Read the delivery manifest beside a folder, or nil plus a reason.
local function readManifest(folder)
    local path = LrPathUtils.child(folder, "aura-delivery-manifest.json")
    if not LrFileUtils.exists(path) then
        return nil, "No aura-delivery-manifest.json in that folder. Point at the folder AURA "
            .. "exported into."
    end
    local text = LrFileUtils.readFile(path)
    local ok, parsed = pcall(Json.decode, text)
    if not ok or type(parsed) ~= "table" then
        return nil, "That delivery manifest could not be read."
    end
    if parsed.schema ~= "aura.delivery-manifest/1" then
        return nil, "That manifest was written by a version of AURA this plugin does not know: "
            .. tostring(parsed.schema)
    end
    return parsed
end

--- The develop settings for one file, from its XMP sidecar.
local function readSidecar(path)
    local sidecar = LrPathUtils.replaceExtension(path, "xmp")
    if not LrFileUtils.exists(sidecar) then
        return nil
    end
    return require("AuraXmp").parse(LrFileUtils.readFile(sidecar))
end

local function importInto(catalog, manifest, folder, major)
    local applied, skippedKeys, missing = 0, {}, 0

    catalog:withWriteAccessDo("Import from AURA", function()
        for _, entry in ipairs(manifest.files or {}) do
            local path = LrPathUtils.child(folder, entry.path)
            local photo = catalog:findPhotoByPath(path)
            if not photo then
                missing = missing + 1
            else
                -- 1. Selections. The expensive half to redo by hand, so it goes first.
                photo:setRawMetadata("pickStatus", 1)

                -- 2. Labels, one per phase 29 set.
                local label = manifest.labels and manifest.labels[entry.path]
                if label then
                    photo:setRawMetadata("colorNameForLabel", label)
                end

                -- 3. Grading, from the sidecar.
                local settings = readSidecar(path)
                if settings then
                    local usable = {}
                    for key, value in pairs(settings) do
                        local floor = DEVELOP_KEYS[key]
                        if floor and major >= floor then
                            usable[key] = value
                        elseif floor then
                            skippedKeys[key] = true
                        end
                    end
                    photo:applyDevelopSettings(usable, "AURA", true)
                    applied = applied + 1
                end
            end
        end
    end)

    return applied, skippedKeys, missing
end

LrTasks.startAsyncTask(function()
    LrFunctionContext.callWithContext("aura-import", function()
        local major = majorVersion()
        if major < 12 then
            LrDialogs.message(
                "AURA needs Lightroom Classic 12 or newer",
                "This copy of Lightroom is version " .. tostring(major) .. ". AURA's selections "
                    .. "can still be brought in by reading the XMP sidecars directly.",
                "warning"
            )
            return
        end

        local folder = LrDialogs.runOpenPanel({
            title = "Choose the folder AURA exported into",
            canChooseFiles = false,
            canChooseDirectories = true,
            allowsMultipleSelection = false,
        })
        if not folder or #folder == 0 then
            return
        end

        local manifest, why = readManifest(folder[1])
        if not manifest then
            LrDialogs.message("AURA could not read that delivery", why, "critical")
            return
        end

        local catalog = LrApplication.activeCatalog()
        local applied, skipped, missing = importInto(catalog, manifest, folder[1], major)

        -- What was *not* done, named. A frame whose curve was skipped and a frame whose curve was
        -- flat must never look the same.
        local lines = { applied .. " photographs graded from AURA." }
        if missing > 0 then
            lines[#lines + 1] = missing .. " files in the manifest are not in this catalog yet. "
                .. "Import them first, then run this again."
        end
        local skippedNames = {}
        for key in pairs(skipped) do
            skippedNames[#skippedNames + 1] = key
        end
        if #skippedNames > 0 then
            table.sort(skippedNames)
            lines[#lines + 1] = "Lightroom " .. major .. " cannot take these settings, so they "
                .. "were left out: " .. table.concat(skippedNames, ", ") .. "."
        end
        LrDialogs.message("AURA import finished", table.concat(lines, "\n\n"), "info")
    end)
end)
