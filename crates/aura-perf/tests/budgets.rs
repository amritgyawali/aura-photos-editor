use std::path::Path;

use aura_core::clock::FixedClock;
use aura_perf::{Budget, Budgets, Measurement, StageTimer};
use time::OffsetDateTime;

fn budgets() -> Budgets {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf/budgets.toml");
    Budgets::load(&path).expect("perf/budgets.toml must parse")
}

#[test]
fn every_phase_01_budget_is_declared() {
    let budgets = budgets();
    for stage in [
        "ingest_4000_files",
        "catalog_open",
        "timeline_recompute_4000",
        "reimport_noop_4000",
    ] {
        assert!(
            budgets.stage.contains_key(stage),
            "{stage} has no budget; a budget in a phase document must become a value here"
        );
    }
}

#[test]
fn a_measurement_inside_its_budget_passes() {
    let budgets = budgets();
    let measurement = Measurement {
        stage: "catalog_open".to_string(),
        elapsed_ms: 120,
        units: 1,
    };
    assert!(budgets.check(&measurement).is_ok());
}

#[test]
fn a_measurement_over_its_budget_fails_with_the_numbers() {
    let budgets = budgets();
    let measurement = Measurement {
        stage: "catalog_open".to_string(),
        elapsed_ms: 900,
        units: 1,
    };
    let breach = budgets.check(&measurement).expect_err("must breach");
    assert!(
        breach.contains("900"),
        "the message must carry the measurement"
    );
    assert!(breach.contains("400"), "the message must carry the budget");
}

#[test]
fn per_unit_budgets_are_enforced_too() {
    let budgets = budgets();
    let measurement = Measurement {
        stage: "ingest_4000_files".to_string(),
        elapsed_ms: 80_000,
        units: 1_000,
    };
    assert!(
        budgets.check(&measurement).is_err(),
        "80 ms per file breaches the 20 ms per-file budget even though the total fits"
    );
}

#[test]
fn an_unbudgeted_stage_is_not_a_failure() {
    let budgets = budgets();
    let measurement = Measurement {
        stage: "not_yet_budgeted".to_string(),
        elapsed_ms: 10_000_000,
        units: 1,
    };
    assert!(budgets.check(&measurement).is_ok());
}

#[test]
fn the_stage_timer_reads_the_clock_exactly_twice() {
    let clock = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    let timer = StageTimer::start("catalog_open", clock.as_ref());
    clock.advance_ms(250);
    let measurement = timer.finish(1);

    assert_eq!(measurement.elapsed_ms, 250);
    assert!((measurement.units_per_second() - 4.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// The host scale, added in PHASE-19 because phase 14's guardrail assumed CI was
// fast and it is not.
// ---------------------------------------------------------------------------

/// A budget of ten milliseconds over one unit.
fn tight() -> Budgets {
    let mut budgets = Budgets::default();
    budgets.stage.insert(
        "probe".to_string(),
        Budget {
            max_elapsed_ms: 10,
            max_ms_per_unit: 10,
        },
    );
    budgets
}

fn measured(ms: u64) -> Measurement {
    Measurement {
        stage: "probe".to_string(),
        elapsed_ms: ms,
        units: 1,
    }
}

#[test]
fn an_unset_host_scale_is_one() {
    // The developer machine, and the default. A variable that had to be set to get the tight
    // assertion would be a variable somebody forgets to set.
    std::env::remove_var(aura_perf::HOST_SCALE_VAR);
    assert_eq!(aura_perf::host_scale(), 1);
    assert!(tight().check(&measured(11)).is_err());
}

#[test]
fn a_scale_relaxes_a_timing_budget_and_names_both_figures() {
    std::env::set_var(aura_perf::HOST_SCALE_VAR, "4");
    assert_eq!(aura_perf::host_scale(), 4);
    assert!(tight().check(&measured(39)).is_ok(), "39 ms is inside 4x10");
    let breach = tight()
        .check(&measured(41))
        .expect_err("41 ms is outside 4x10");
    assert!(
        breach.contains("40"),
        "the allowed figure is missing: {breach}"
    );
    assert!(
        breach.contains("10") && breach.contains("host scale 4"),
        "the stated figure and the scale are missing: {breach}"
    );
    std::env::remove_var(aura_perf::HOST_SCALE_VAR);
}

#[test]
fn a_scale_never_relaxes_a_size_a_count_or_a_cost() {
    // The property that keeps this from becoming a way to make any red build green. A slow
    // runner is not a reason to store more, call more or spend more.
    std::env::set_var(aura_perf::HOST_SCALE_VAR, "8");
    let mut budgets = Budgets::default();
    budgets.size.insert(
        "bytes".to_string(),
        aura_perf::SizeBudget { max_bytes: 100 },
    );
    budgets.count.insert(
        "calls".to_string(),
        aura_perf::CountBudget { max_count: 10 },
    );
    budgets.cost.insert(
        "spend".to_string(),
        aura_perf::CostBudget {
            max_usd_hundredths_of_a_cent: 100,
        },
    );
    assert!(budgets.check_size("bytes", 101).is_err());
    assert!(budgets.check_count("calls", 11).is_err());
    assert!(budgets.check_cost("spend", 0.011).is_err());
    std::env::remove_var(aura_perf::HOST_SCALE_VAR);
}

#[test]
fn a_scale_cannot_switch_a_budget_off() {
    // A budget that can be disabled from the environment is not a budget. Anything above the
    // ceiling clamps, and anything unparseable tightens rather than loosens.
    std::env::set_var(aura_perf::HOST_SCALE_VAR, "1000");
    assert_eq!(aura_perf::host_scale(), aura_perf::MAX_HOST_SCALE);
    for nonsense in ["", "  ", "later", "-4", "3.5"] {
        std::env::set_var(aura_perf::HOST_SCALE_VAR, nonsense);
        assert_eq!(
            aura_perf::host_scale(),
            1,
            "`{nonsense}` should have tightened to 1"
        );
    }
    std::env::remove_var(aura_perf::HOST_SCALE_VAR);
}
