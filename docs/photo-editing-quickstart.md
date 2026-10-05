# Import, edit and export photographs

## Open AURA on Windows

Open **AURA Photo Editor** from the Desktop or Start menu, or double-click
**Start AURA.cmd** in the repository folder. The app runs with its interface
bundled inside; no terminal commands or development server are needed for
subsequent launches. Clicking again brings the existing window forward.

To recreate the shortcuts, run `& '.\Start AURA.cmd' -InstallShortcuts` in
PowerShell. Keep the repository folder in place because the shortcuts point to it.
When `app/aura-desktop.exe` is installed, the launcher opens that tested bundle.
Use `-Rebuild` to replace it after changing source. Without an installed bundle,
the launcher builds the app if it is missing or its source files have changed,
using Node.js, Rust and the Windows C++ build tools described in the README.
After source changes, close AURA and open the shortcut again. To force a rebuild,
run `& '.\Start AURA.cmd' -Rebuild`. Build logs are in `.work-checks/launcher`;
runtime logs are in `%APPDATA%\AURA\logs`.

## The permanent studio workspace

AURA always opens the plum studio with **Start, Photos, Auto edit, Instagram
style, Export, Advanced** in its sidebar. System theme and old saved theme
preferences do not replace this layout. Collections stay in the sidebar.

On Start, choose a look, optionally add a reference, then choose photographs or
a folder. Import starts local automatic editing. In **Auto edit**, choose all or
selected photos; each keeps its own saved retouch preferences and manual edits.
Use **Export** to select a destination and render the saved results.

Extra tools (camera matching, culling, portrait regions, object cleanup, provider
setup and diagnostics) live under **Advanced** in the same studio interface.
The complete collection workflow is also available there when needed.

Portrait retouch uses each photo's saved scope, strength and fine controls, or
natural defaults for a new photo. Photos without detected people receive no
portrait operations. The complete collection workflow also runs this local
portrait pass after light/color correction and before verified export.

## Edit individual photographs

1. Run the desktop application, create a collection, and open it.
2. Choose **Choose photos** or **Choose a folder** to import and process. JPEG
   and PNG work directly; camera RAW support depends on the camera/encoding.
3. Select a photograph in Photos, then open **Auto edit**. **Auto edit photo**
   adjusts one photo; **Auto edit all photos** processes every imported photo
   sequentially. Stop preserves completed edits.
4. Use **Show original** to compare. Adjust exposure, contrast, highlights,
   shadows and vibrance manually, or undo/reset. Automatic edits respect fields
   you have set yourself.
5. In delivery, choose an export folder and a preset, preview the filenames,
   then export. This exports the imported photographs, including a first export
   before any culling pass. Original files are never overwritten.

## AI connection

Choose a vision-capable provider and model in **AI setup**, save the provider key
in the app, and enable cloud AI with project consent in AI settings. The existing
gateway enforces consent, budget and offline settings. Ollama/LM Studio are also
available when a compatible vision model is running locally.

Auto edit sends a small metadata-free derivative to the configured model and
applies validated global adjustments through AURA's local renderer. It does not
replace people or generate new scene content. The result reports the model and
its reasons. If no provider can answer, it clearly says **Local enhancement**:
that uses conservative exposure, tonal and color measurements, not a trained AI model.
Review intentional silhouettes, night scenes and high-key photographs.

The bundled advanced face, masking, culling and retouch models remain subject to
the limitations in the phase review. Adding a cloud provider does not train or
validate those local models. This workflow does not claim production readiness
for unattended wedding delivery or support for every RAW camera format.

## Development

From the repository root:

```powershell
cd ui
npm run build
cd ..
cargo build --locked --manifest-path ui/src-tauri/Cargo.toml --target-dir target/desktop-launch --features custom-protocol -j 1
& .\target\desktop-launch\debug\aura-desktop.exe
```

The default catalog persists in the current Windows user's AURA application data
folder. Preview caches are disposable; keep the originals in their imported locations.

## Deep acne cleanup

Open **Auto edit > Retouch**, select **Deep acne cleanup**, and run automatic
retouch. This opt-in preset includes compact dark marks and can affect freckles
or beauty marks. Review the selection and saved operations before export.
Frequency separation can retain fine texture while smoothing broader unevenness;
healing borrows nearby skin texture. Every operation stays editable and undoable.
No automatic pass guarantees complete skin selection or removal of every mark.
