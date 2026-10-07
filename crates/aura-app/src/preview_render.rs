//! Disposable edited previews. Never used by analysis or delivery. ADR-0095.
use std::collections::VecDeque;

use aura_core::AuraResult;
use aura_render::{RenderLevel, RenderPurpose, RenderRequest, RenderService, RenderedImage};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct PreviewRenders {
    entries: VecDeque<(String, RenderedImage)>,
    bytes: usize,
}

fn size(image: &RenderedImage) -> usize {
    match &image.data {
        aura_render::RenderedData::Eight(bytes) => bytes.len(),
        aura_render::RenderedData::Sixteen(words) => words.len().saturating_mul(2),
    }
}

impl PreviewRenders {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn get(&mut self, key: &str) -> Option<RenderedImage> {
        let index = self.entries.iter().position(|(saved, _)| saved == key)?;
        let entry = self.entries.remove(index)?;
        let mut image = entry.1.clone();
        image.cache_hit = true;
        image.ms = 0;
        self.entries.push_back(entry);
        Some(image)
    }

    fn insert(&mut self, key: String, image: &RenderedImage) {
        let bytes = size(image);
        if bytes > MAX_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(saved, _)| *saved == key) {
            if let Some((_, previous)) = self.entries.remove(index) {
                self.bytes = self.bytes.saturating_sub(size(&previous));
            }
        }
        while self.bytes.saturating_add(bytes) > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let Some((_, previous)) = self.entries.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(size(&previous));
        }
        self.bytes += bytes;
        self.entries.push_back((key, image.clone()));
    }
}

/// Fit display requests before decoding/processing. Stored coordinates stay relative to
/// the original; export and analysis bypass both this limit and the edited cache.
pub(crate) fn render(
    state: &crate::AppState,
    mut request: RenderRequest,
) -> AuraResult<RenderedImage> {
    let engine = state.render()?;
    if request.purpose != RenderPurpose::Interactive {
        return engine.render(request);
    }
    aura_recipe::schema::Validation::check(&request.recipe)?;
    let edge = request
        .level
        .long_edge()
        .unwrap_or(aura_render::cpu::INTERACTIVE_PREVIEW_EDGE)
        .clamp(1, aura_render::cpu::INTERACTIVE_PREVIEW_EDGE);
    request.level = RenderLevel::Screen(edge, edge);
    // The hash includes source content, complete recipe and output spec. Size is additional:
    // the existing renderer hash intentionally does not include the requested resolution.
    let key = format!(
        "{}:{edge}:{}",
        request.image_id.to_db(),
        engine.render_hash(&request)?
    );
    if let Some(image) = state.preview_renders.lock().get(&key) {
        return Ok(image);
    }
    let image = engine.render(request)?;
    // Never hold the cache lock while rendering another photograph.
    state.preview_renders.lock().insert(key, &image);
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_render::{OutputSpec, RenderedData};

    #[test]
    fn edited_cache_is_bounded_and_refreshes_recently_used_entries() {
        let mut cache = PreviewRenders::default();
        let image = RenderedImage {
            width: 1,
            height: 1,
            data: RenderedData::Eight(vec![1, 2, 3]),
            colour_space: OutputSpec::default().colour_space,
            render_hash: String::new(),
            backend: "cpu".into(),
            notes: vec![],
            stages_run: vec![],
            ms: 10,
            cache_hit: false,
        };
        for number in 0..MAX_ENTRIES {
            cache.insert(number.to_string(), &image);
        }
        assert!(cache.get("0").unwrap().cache_hit);
        cache.insert("new".into(), &image);
        assert!(cache.get("1").is_none());
        assert!(cache.get("0").is_some());
        let mut oversized = image;
        oversized.data = RenderedData::Eight(vec![0; MAX_BYTES + 1]);
        cache.insert("oversized".into(), &oversized);
        assert!(cache.get("oversized").is_none());
        assert!(cache.bytes <= MAX_BYTES);
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert!(cache.entries.is_empty());
        oversized.data = RenderedData::Eight(vec![0; MAX_BYTES / 3]);
        for number in 0..5 {
            cache.insert(number.to_string(), &oversized);
        }
        assert!(cache.bytes <= MAX_BYTES);
        assert_eq!(
            cache.entries.len(),
            3,
            "pixel budget evicts before entry count"
        );
    }
}
