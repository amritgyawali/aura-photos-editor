//! Read every typed IPC wrapper for the phase verification gates.
use std::path::Path;

pub(crate) fn client_sources() -> Result<String, String> {
    let root = Path::new("ui/src/ipc");
    let entries = std::fs::read_dir(root)
        .map_err(|err| format!("{} could not be listed: {err}", root.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|err| format!("{} could not be listed: {err}", root.display()))?;
    paths.retain(|path| {
        path.extension().is_some_and(|extension| extension == "ts")
            && !path.to_string_lossy().ends_with(".test.ts")
    });
    paths.sort();
    let mut sources = String::new();
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|err| format!("{} could not be read: {err}", path.display()))?;
        sources.push_str(&source);
        sources.push('\n');
    }
    Ok(sources)
}
