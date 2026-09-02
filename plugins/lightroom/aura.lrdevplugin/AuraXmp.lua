--[[
  Read the develop settings out of an XMP sidecar.

  `aura_recipe::xmp::write` produces these, and this parses the same subset back. Deliberately a
  reader and not a writer: this plugin never writes an XMP, because a sidecar beside a RAW is the
  RAW's own record and two writers of it is two answers to what the edit was.
--]]

local Xmp = {}

-- The `crs:` attributes AURA writes, mapped onto Lightroom's develop keys.
local KEYS = {
    Exposure2012 = "Exposure2012",
    Temperature = "Temperature",
    Tint = "Tint",
    Contrast2012 = "Contrast2012",
    Highlights2012 = "Highlights2012",
    Shadows2012 = "Shadows2012",
    Whites2012 = "Whites2012",
    Blacks2012 = "Blacks2012",
    Vibrance = "Vibrance",
    Saturation = "Saturation",
}

--- Parse a sidecar into a develop-settings table, or nil when it carries none.
function Xmp.parse(text)
    if type(text) ~= "string" then
        return nil
    end
    local out, found = {}, false
    for attribute, key in pairs(KEYS) do
        -- Both forms an XMP writer produces: an attribute on the description element, and a child
        -- element. Reading only the first is how a plugin comes to work with one exporter and not
        -- another.
        local value = text:match('crs:' .. attribute .. '="([^"]*)"')
            or text:match('<crs:' .. attribute .. '>([^<]*)</crs:' .. attribute .. '>')
        local number = value and tonumber(value)
        if number then
            out[key] = number
            found = true
        end
    end
    if not found then
        return nil
    end
    return out
end

return Xmp
