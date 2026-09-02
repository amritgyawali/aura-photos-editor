--[[
  A JSON encoder and decoder, by hand.

  Lightroom's SDK ships `LrPathUtils`, `LrFileUtils` and a dozen other modules and no JSON. Every
  Lightroom plugin in circulation therefore vendors one, and this is that - kept to the subset the
  two documents this plugin reads and writes actually contain: objects, arrays, strings, numbers,
  booleans and null.

  What it deliberately does not do is accept everything. A decoder that recovered from a malformed
  document would turn a truncated manifest into a partial import, which is a photographer told
  their wedding arrived when four chapters of it did not.
--]]

local Json = {}

local function encodeString(s)
    local out = s:gsub('[%c"\\]', function(c)
        if c == '"' then return '\\"' end
        if c == "\\" then return "\\\\" end
        if c == "\n" then return "\\n" end
        if c == "\r" then return "\\r" end
        if c == "\t" then return "\\t" end
        return string.format("\\u%04x", c:byte())
    end)
    return '"' .. out .. '"'
end

local function isArray(t)
    local n = 0
    for _ in pairs(t) do n = n + 1 end
    return n == #t
end

function Json.encode(value)
    local kind = type(value)
    if value == nil then return "null" end
    if kind == "boolean" then return tostring(value) end
    if kind == "number" then return string.format("%.6g", value) end
    if kind == "string" then return encodeString(value) end
    if kind ~= "table" then
        error("cannot encode a " .. kind)
    end

    local parts = {}
    if isArray(value) then
        for _, v in ipairs(value) do
            parts[#parts + 1] = Json.encode(v)
        end
        return "[" .. table.concat(parts, ",") .. "]"
    end

    -- Keys in sorted order, so two identical exports produce identical documents. The same
    -- discipline `aura_export::manifest` keeps, and for the same reason: a document whose bytes
    -- move between runs cannot be compared.
    local keys = {}
    for k in pairs(value) do keys[#keys + 1] = k end
    table.sort(keys)
    for _, k in ipairs(keys) do
        parts[#parts + 1] = encodeString(tostring(k)) .. ":" .. Json.encode(value[k])
    end
    return "{" .. table.concat(parts, ",") .. "}"
end

local Decoder = {}
Decoder.__index = Decoder

local function newDecoder(text)
    return setmetatable({ text = text, at = 1 }, Decoder)
end

function Decoder:skip()
    local _, to = self.text:find("^[ \t\r\n]*", self.at)
    self.at = to + 1
end

function Decoder:expect(ch)
    if self.text:sub(self.at, self.at) ~= ch then
        error("expected " .. ch .. " at " .. self.at)
    end
    self.at = self.at + 1
end

function Decoder:value()
    self:skip()
    local ch = self.text:sub(self.at, self.at)
    if ch == "{" then return self:object() end
    if ch == "[" then return self:array() end
    if ch == '"' then return self:str() end
    if ch == "t" then self.at = self.at + 4; return true end
    if ch == "f" then self.at = self.at + 5; return false end
    if ch == "n" then self.at = self.at + 4; return nil end
    return self:number()
end

function Decoder:object()
    self:expect("{")
    local out = {}
    self:skip()
    if self.text:sub(self.at, self.at) == "}" then
        self.at = self.at + 1
        return out
    end
    while true do
        self:skip()
        local key = self:str()
        self:skip()
        self:expect(":")
        out[key] = self:value()
        self:skip()
        local ch = self.text:sub(self.at, self.at)
        self.at = self.at + 1
        if ch == "}" then return out end
        if ch ~= "," then error("expected , or } at " .. self.at) end
    end
end

function Decoder:array()
    self:expect("[")
    local out = {}
    self:skip()
    if self.text:sub(self.at, self.at) == "]" then
        self.at = self.at + 1
        return out
    end
    while true do
        out[#out + 1] = self:value()
        self:skip()
        local ch = self.text:sub(self.at, self.at)
        self.at = self.at + 1
        if ch == "]" then return out end
        if ch ~= "," then error("expected , or ] at " .. self.at) end
    end
end

function Decoder:str()
    self:expect('"')
    local out = {}
    while true do
        local ch = self.text:sub(self.at, self.at)
        self.at = self.at + 1
        if ch == '"' then return table.concat(out) end
        if ch == "" then error("unterminated string") end
        if ch == "\\" then
            local esc = self.text:sub(self.at, self.at)
            self.at = self.at + 1
            if esc == "n" then out[#out + 1] = "\n"
            elseif esc == "t" then out[#out + 1] = "\t"
            elseif esc == "r" then out[#out + 1] = "\r"
            elseif esc == "u" then
                local hex = self.text:sub(self.at, self.at + 3)
                self.at = self.at + 4
                out[#out + 1] = string.char(tonumber(hex, 16) % 256)
            else out[#out + 1] = esc end
        else
            out[#out + 1] = ch
        end
    end
end

function Decoder:number()
    local from, to = self.text:find("^-?%d+%.?%d*[eE]?[-+]?%d*", self.at)
    if not from then error("expected a number at " .. self.at) end
    self.at = to + 1
    return tonumber(self.text:sub(from, to))
end

function Json.decode(text)
    local d = newDecoder(text)
    local value = d:value()
    d:skip()
    if d.at <= #text then
        -- Trailing content means the document is not what it claims to be. Refusing is the point:
        -- a truncated manifest that decoded to its first half would be a partial import reported
        -- as a whole one.
        error("trailing content at " .. d.at)
    end
    return value
end

return Json
