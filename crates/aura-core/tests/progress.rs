use aura_core::progress::{CancelToken, ProgressCounter, ProgressSink, ProgressUpdate};
use parking_lot::Mutex;

#[derive(Debug, Default)]
struct RecordingSink {
    seen: Mutex<Vec<ProgressUpdate>>,
}

impl ProgressSink for RecordingSink {
    fn report(&self, update: ProgressUpdate) {
        self.seen.lock().push(update);
    }
}

#[test]
fn cancel_is_visible_to_every_clone() {
    let token = CancelToken::new();
    let worker_copy = token.clone();
    assert!(!worker_copy.is_cancelled());
    token.cancel();
    assert!(
        worker_copy.is_cancelled(),
        "cancel must be visible across clones"
    );
}

#[test]
fn counter_tracks_fraction_and_clamps() {
    let counter = ProgressCounter::with_total(4);
    assert!(counter.fraction() < f64::EPSILON);
    let _ = counter.advance(2);
    assert!((counter.fraction() - 0.5).abs() < 1e-9);
    let _ = counter.advance(10);
    assert!(
        (counter.fraction() - 1.0).abs() < 1e-9,
        "fraction must clamp at one"
    );
}

#[test]
fn unknown_total_reports_zero_rather_than_dividing_by_zero() {
    let counter = ProgressCounter::default();
    let _ = counter.advance(7);
    assert!(counter.fraction() < f64::EPSILON);
    assert_eq!(counter.done(), 7);
}

#[test]
fn sink_receives_updates_in_order() {
    let sink = RecordingSink::default();
    for done in 1..=3 {
        sink.report(ProgressUpdate {
            stage: "ingest.hash",
            done,
            total: 3,
            current: None,
        });
    }
    let seen = sink.seen.lock();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen.iter().map(|u| u.done).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}
