//! The memory ledger, the gate and the cancellation flag, on their own.
//!
//! `runtime.rs` proves these three work inside the engine; this file proves each
//! one in isolation, so a failure says which of them broke.

use std::sync::Arc;

use aura_core::clock::FixedClock;
use aura_core::{AuraResult, Priority};
use aura_infer::batch::{chunk_ranges, BatchScheduler, CancelToken, MemoryLedger};
use time::OffsetDateTime;

fn scheduler(budget_bytes: u64) -> BatchScheduler {
    let clock = FixedClock::at(OffsetDateTime::UNIX_EPOCH);
    BatchScheduler::new(clock, Arc::new(MemoryLedger::new(budget_bytes)))
}

#[test]
fn the_ledger_refuses_before_the_budget_is_exceeded() {
    let ledger = Arc::new(MemoryLedger::new(1000));
    let first = ledger.reserve(600).expect("first fits");
    assert!(ledger.reserve(600).is_none(), "second must not fit");
    drop(first);
    assert!(ledger.reserve(600).is_some(), "space is released on drop");
    assert_eq!(ledger.refusals(), 1);
}

#[test]
fn a_reservation_is_released_even_when_the_work_fails() {
    let scheduler = scheduler(1000);
    let cancel = CancelToken::new();
    let outcome: AuraResult<()> =
        scheduler.admit(Priority::AiBatch, 900, "model", 1, &cancel, || {
            Err(aura_core::errors::ml::cancelled("deliberate"))
        });
    assert!(outcome.is_err());
    assert_eq!(scheduler.ledger().used_bytes(), 0);
}

#[test]
fn work_that_does_not_fit_is_told_to_halve_rather_than_refused() {
    let scheduler = scheduler(100);
    let cancel = CancelToken::new();
    let outcome: AuraResult<()> =
        scheduler.admit(Priority::AiBatch, 200, "model", 8, &cancel, || Ok(()));
    let err = outcome.expect_err("must not fit");
    assert_eq!(err.code.0, "AURA-GPU-4003");
    assert_eq!(scheduler.stats().downshifts, 1);
}

#[test]
fn a_cancelled_token_stops_work_before_it_starts() {
    let scheduler = scheduler(0);
    let cancel = CancelToken::new();
    cancel.cancel();
    let outcome: AuraResult<()> =
        scheduler.admit(Priority::AiBatch, 10, "model", 1, &cancel, || Ok(()));
    assert_eq!(outcome.expect_err("cancelled").code.0, "AURA-ML-5011");
}

#[test]
fn an_unbudgeted_ledger_still_reports_its_peak() {
    let ledger = Arc::new(MemoryLedger::new(0));
    let held = ledger.reserve(4096).expect("unbudgeted always fits");
    assert_eq!(ledger.peak_bytes(), 4096);
    drop(held);
    assert_eq!(ledger.used_bytes(), 0);
}

#[test]
fn the_budget_is_seventy_per_cent_of_what_a_device_reports_free() {
    // Integer arithmetic, so the documented number and the computed one agree
    // exactly rather than to within a float rounding error.
    let ledger = MemoryLedger::from_free_mb(1000);
    let free_bytes = 1000u64 * 1024 * 1024;
    assert_eq!(ledger.budget_bytes(), free_bytes / 100 * 70);
}

#[test]
fn chunking_covers_every_item_exactly_once() {
    assert_eq!(chunk_ranges(5, 2), vec![(0, 2), (2, 4), (4, 5)]);
    assert_eq!(chunk_ranges(0, 4), vec![]);
    assert_eq!(chunk_ranges(3, 0), vec![(0, 1), (1, 2), (2, 3)]);
}
