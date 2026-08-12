use std::path::Path;

use aura_core::clock::FixedClock;
use aura_perf::{Budgets, Measurement, StageTimer};
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
