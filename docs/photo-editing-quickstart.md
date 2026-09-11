# Import, edit and export photographs

1. Run the desktop application, create a wedding/project, and open it.
2. Choose **Choose photos** or **Choose folders**, then **Start import**. JPEG
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
that is a conservative brightness-histogram adjustment, not a trained AI model.
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
cargo build --manifest-path ui/src-tauri/Cargo.toml --features custom-protocol
& .\ui\src-tauri\target\debug\aura-desktop.exe
```

The default catalog persists in the current Windows user's AURA application data
folder. Preview caches are disposable; keep the originals in their imported locations.
