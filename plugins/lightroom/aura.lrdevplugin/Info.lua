--[[
  AURA for Lightroom Classic. PHASE-30 section 6.2.

  The version floor is a *detection* rather than a hope. Lightroom refuses to load a plugin whose
  `LrSdkMinimumVersion` it cannot meet, which is how a photographer on Lightroom 11 finds out
  something is wrong at install time rather than half-way through a wedding.

  `LrSdkMinimumVersion = 6.0` is Lightroom Classic 12. Below that the develop-settings API this
  plugin writes through has a different shape, and a plugin that loaded anyway would import
  selections correctly and grading incorrectly - which is the worst of the three outcomes, because
  it looks like it worked.
--]]

return {
    LrSdkVersion = 13.0,
    LrSdkMinimumVersion = 6.0,

    LrToolkitIdentifier = "com.aura.wedding",
    LrPluginName = "AURA Wedding AI",
    LrPluginInfoUrl = "https://example.invalid/aura",

    LrExportMenuItems = {
        {
            title = "Export corrections to AURA",
            file = "AuraExportCorrections.lua",
        },
    },

    LrLibraryMenuItems = {
        {
            title = "Import AURA selections and grading",
            file = "AuraImport.lua",
        },
    },

    VERSION = { major = 1, minor = 0, revision = 0 },
}
