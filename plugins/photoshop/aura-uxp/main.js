/*
 * Open an AURA delivery in Photoshop: pixels, regions as layer masks, retouch as its own layer.
 *
 * ## What this file is careful about
 *
 * **It reads the manifest and refuses an unknown schema.** A folder that is not an AURA delivery,
 * or one written by a newer release, is named rather than guessed at. A plugin that opened whatever
 * TIFFs it found would open a photographer's originals as a delivery.
 *
 * **It degrades by version rather than by exception.** On Photoshop 23 the layer-mask API this uses
 * is not there; rather than throwing half-way through a 700-frame open, the version is checked
 * first and the flattened path is taken with a message that says so.
 *
 * **It never writes back.** There is no code path from here into an AURA catalog. A PSD is a fork:
 * the moment a layer is flattened, the four values phase 14 needs to re-create the file no longer
 * describe it.
 */

const { app, core } = require("photoshop");
const fs = require("uxp").storage.localFileSystem;

/** The manifest schema this plugin knows. */
const SCHEMA = "aura.delivery-manifest/1";

/** The lowest Photoshop that has the layer APIs the full path needs. */
const LAYERED_FROM = 24;

function hostMajor() {
  const parts = String(app.version || "0").split(".");
  return Number.parseInt(parts[0], 10) || 0;
}

function say(text) {
  const el = document.getElementById("status");
  if (el) {
    el.textContent = text;
  }
}

async function readManifest(folder) {
  const entries = await folder.getEntries();
  const found = entries.find((e) => e.name === "aura-delivery-manifest.json");
  if (!found) {
    throw new Error(
      "There is no aura-delivery-manifest.json in that folder. Choose the folder AURA exported into."
    );
  }
  const text = await found.read();
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (_e) {
    throw new Error("That delivery manifest could not be read.");
  }
  if (parsed.schema !== SCHEMA) {
    throw new Error(
      `That manifest was written by a version of AURA this plugin does not know: ${parsed.schema}`
    );
  }
  return parsed;
}

/**
 * The regions beside one delivered file, when the export wrote any.
 *
 * `null` rather than an empty list when there are none: "this frame has no regions" and "this
 * build did not export regions" are different facts, and only the second is worth telling
 * somebody about.
 */
async function readRegions(folder, relPath) {
  const stem = relPath.replace(/\.[^./]+$/, "");
  try {
    const entry = await folder.getEntry(`${stem}.regions.json`);
    return JSON.parse(await entry.read());
  } catch (_e) {
    return null;
  }
}

async function openLayered(folder, entry) {
  const file = await folder.getEntry(entry.path);
  const doc = await app.open(file);

  const regions = await readRegions(folder, entry.path);
  if (!regions || !Array.isArray(regions.regions)) {
    return { doc, masks: 0, retouch: false };
  }

  let masks = 0;
  for (const region of regions.regions) {
    // One layer per region, its alpha as the mask. A retoucher works on AURA's boundary rather
    // than redrawing it, which is the whole value of the hand-off.
    const layer = await doc.createLayer({ name: `AURA ${region.kind}` });
    if (layer && region.mask_path) {
      const maskFile = await folder.getEntry(region.mask_path);
      await core.executeAsModal(
        async () => {
          await layer.applyLayerMaskFromFile(maskFile);
        },
        { commandName: `AURA ${region.kind} mask` }
      );
      masks += 1;
    }
  }

  // The retouch, above the base and switchable. A retoucher who cannot turn AURA's skin work off
  // cannot compare it, and a retoucher who cannot compare it will redo it.
  let retouch = false;
  if (regions.retouch_path) {
    const retouchFile = await folder.getEntry(regions.retouch_path);
    await core.executeAsModal(
      async () => {
        await doc.createLayer({ name: "AURA retouch", fromFile: retouchFile });
      },
      { commandName: "AURA retouch layer" }
    );
    retouch = true;
  }

  return { doc, masks, retouch };
}

async function run() {
  const major = hostMajor();
  const folder = await fs.getFolder();
  if (!folder) {
    return;
  }

  let manifest;
  try {
    manifest = await readManifest(folder);
  } catch (e) {
    say(e.message);
    return;
  }

  const files = manifest.files || [];
  if (files.length === 0) {
    say("That delivery has no files in it.");
    return;
  }

  if (major < LAYERED_FROM) {
    // Degrade rather than throw. A 700-frame open that fails on the first frame is worse than one
    // that says up front what it cannot do.
    say(
      `Photoshop ${major} cannot take AURA's layer masks, so these will open flattened. ` +
        `Photoshop ${LAYERED_FROM} or newer opens them with regions and retouch on their own layers.`
    );
    for (const entry of files) {
      const file = await folder.getEntry(entry.path);
      await app.open(file);
    }
    return;
  }

  let opened = 0;
  let masks = 0;
  let retouched = 0;
  for (const entry of files) {
    try {
      const result = await openLayered(folder, entry);
      opened += 1;
      masks += result.masks;
      if (result.retouch) {
        retouched += 1;
      }
    } catch (_e) {
      // One frame that will not open is one frame. The rest of the delivery still opens, and the
      // count at the end says how many did not - the same shape `aura-export` takes when a frame
      // will not render.
    }
  }

  const parts = [`${opened} of ${files.length} photographs opened.`];
  if (masks > 0) {
    parts.push(`${masks} region masks.`);
  }
  if (retouched > 0) {
    parts.push(`${retouched} with AURA's retouch on its own layer.`);
  }
  if (masks === 0) {
    parts.push(
      "This delivery carries no region files, so nothing is masked. That is what an export " +
        "without regions looks like, not a failure."
    );
  }
  say(parts.join(" "));
}

document.getElementById("open").addEventListener("click", () => {
  run().catch((e) => say(e.message));
});
