# Import, edit and export photographs

## Open AURA on Windows

Open **AURA Photo Editor** from the Desktop or Start menu, or double-click
**Start AURA.cmd** in the repository folder. The app runs with its interface
bundled inside; no terminal commands or development server are needed for
subsequent launches. Clicking again brings the existing window forward.

To recreate the shortcuts, run `& '.\Start AURA.cmd' -InstallShortcuts` in
PowerShell. Keep the repository folder in place because the shortcuts point to it.
The launcher builds the app if it is missing or its source files have changed,
using Node.js, Rust and the Windows C++ build tools described in the README.
After source changes, close AURA and open the shortcut again. To force a rebuild,
run `& '.\Start AURA.cmd' -Rebuild`. Build logs are in `.work-checks/launcher`;
runtime logs are in `%APPDATA%\AURA\logs`.

## Finish a project with one button

Choose **Choose photos** or **Choose folders** on the welcome screen or Import
step. Selection starts import, pixel analysis, individual automatic edits and
verified export. The output appears in a unique folder under **Pictures / AURA
Exports**; its full path and progress stay visible while you change tools. Keep
AURA open until the run finishes. **Stop automatic processing** preserves work
already completed and waits for any active export to finish.

Create/open a wedding, import your photographs, and select an output folder in
**Finish everything**. Press **Finish everything** to run the existing analysis,
framing, culling, automatic editing and export pipeline. Progress, local fallbacks
and exported file counts appear beneath the button. Cloud editing requires your
configured provider; local enhancement works without an API key.

## Edit individual photographs

1. Run the desktop application, create a wedding/project, and open it.
2. Choose **Choose photos** or **Choose folders** to import and process. JPEG
   and PNG work directly; camera RAW support depends on the camera/encoding.
3. Select a photograph in Library, then open **Develop**. **Auto edit photo**
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
