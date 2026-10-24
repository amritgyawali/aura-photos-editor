//! Computational budgets for reference fitting. Both modes retain native apply safeguards.
use serde::{Deserialize, Serialize};

/// Natural estimates a photographic look; Palette explicitly requests distribution matching.
/// Neither can recover an unknown editing operation from an unrelated photograph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchIntent {
    #[default]
    Natural,
    Palette,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchQuality {
    Quick,
    #[default]
    Precise,
}

impl MatchQuality {
    pub(super) const fn fitting_edge(self) -> u32 {
        match self {
            Self::Quick => 256,
            Self::Precise => 512,
        }
    }

    pub(super) const fn target_samples(self) -> usize {
        match self {
            Self::Quick => 10_000,
            Self::Precise => 40_000,
        }
    }

    pub(super) const fn iterations(self) -> usize {
        match self {
            Self::Quick => 2,
            Self::Precise => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_quality_keeps_existing_precise_analysis_and_unknown_modes_are_refused() {
        let input: super::super::PrepareInput = serde_json::from_str(
            r#"{"photoId":"photo","referenceId":"reference","cancelId":"cancel"}"#,
        )
        .unwrap();
        assert_eq!(input.quality, MatchQuality::Precise);
        assert_eq!(input.intent, MatchIntent::Natural);
        assert!(serde_json::from_str::<MatchIntent>(r#""perfect""#).is_err());
        assert!(serde_json::from_str::<MatchQuality>(r#""extreme""#).is_err());
        assert!(MatchQuality::Quick.fitting_edge() < MatchQuality::Precise.fitting_edge());
        assert!(MatchQuality::Quick.target_samples() < MatchQuality::Precise.target_samples());
        assert!(MatchQuality::Quick.iterations() < MatchQuality::Precise.iterations());
        let older: super::super::FitDiagnostics = serde_json::from_str(
            r#"{"engine":"reference-grade-transfer-v3","refinementCandidates":4,"uniqueRenders":2,"cachedProposals":0}"#,
        ).unwrap();
        assert_eq!(older.quality, MatchQuality::Precise);
        assert_eq!(older.intent, MatchIntent::Natural);
    }
}
