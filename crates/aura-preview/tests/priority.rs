//! The scheduler. These are latency promises expressed as tests.

use aura_preview::priority::{PriorityQueue, DEFAULT_CAPACITY};
use aura_preview::{ImageId, Priority};
use aura_raw::contract::pixels::PixelLevel;

fn id() -> ImageId {
    ImageId::new()
}

#[test]
fn visible_work_overtakes_a_batch() {
    let mut queue = PriorityQueue::default();
    let batch: Vec<ImageId> = (0..10).map(|_| id()).collect();
    for image in &batch {
        queue.push(*image, PixelLevel::Proxy2048, Priority::AiBatch);
    }
    let urgent = id();
    queue.push(urgent, PixelLevel::Thumb(512), Priority::Visible);

    let first = queue.pop().expect("a request is waiting");
    assert_eq!(first.id, urgent);
    assert_eq!(first.priority, Priority::Visible);
}

#[test]
fn the_classes_are_served_strictly_in_order() {
    let mut queue = PriorityQueue::default();
    let background = id();
    let batch = id();
    let interactive = id();
    let visible = id();
    queue.push(background, PixelLevel::Proxy2048, Priority::Background);
    queue.push(batch, PixelLevel::Proxy2048, Priority::AiBatch);
    queue.push(interactive, PixelLevel::Proxy2048, Priority::Interactive);
    queue.push(visible, PixelLevel::Proxy2048, Priority::Visible);

    let order: Vec<ImageId> = std::iter::from_fn(|| queue.pop())
        .map(|request| request.id)
        .collect();
    assert_eq!(order, vec![visible, interactive, batch, background]);
}

#[test]
fn requests_within_a_class_are_served_in_submission_order() {
    let mut queue = PriorityQueue::default();
    let images: Vec<ImageId> = (0..5).map(|_| id()).collect();
    for image in &images {
        queue.push(*image, PixelLevel::Proxy2048, Priority::AiBatch);
    }
    let served: Vec<ImageId> = std::iter::from_fn(|| queue.pop())
        .map(|request| request.id)
        .collect();
    assert_eq!(served, images);
}

#[test]
fn the_same_work_is_never_queued_twice() {
    let mut queue = PriorityQueue::default();
    let image = id();
    assert!(queue.push(image, PixelLevel::Proxy2048, Priority::AiBatch));
    assert!(!queue.push(image, PixelLevel::Proxy2048, Priority::AiBatch));
    assert_eq!(queue.len(), 1);
}

#[test]
fn different_levels_of_the_same_image_are_different_work() {
    let mut queue = PriorityQueue::default();
    let image = id();
    assert!(queue.push(image, PixelLevel::Thumb(512), Priority::Visible));
    assert!(queue.push(image, PixelLevel::Proxy2048, Priority::Visible));
    assert_eq!(queue.len(), 2);
}

#[test]
fn re_requesting_at_a_higher_priority_promotes_rather_than_duplicates() {
    let mut queue = PriorityQueue::default();
    let image = id();
    queue.push(image, PixelLevel::Proxy2048, Priority::Background);
    assert!(queue.push(image, PixelLevel::Proxy2048, Priority::Visible));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.len_of(Priority::Visible), 1);
    assert_eq!(queue.len_of(Priority::Background), 0);
}

#[test]
fn re_requesting_at_a_lower_priority_changes_nothing() {
    let mut queue = PriorityQueue::default();
    let image = id();
    queue.push(image, PixelLevel::Proxy2048, Priority::Visible);
    assert!(!queue.push(image, PixelLevel::Proxy2048, Priority::Background));
    assert_eq!(queue.len_of(Priority::Visible), 1);
    assert_eq!(queue.len_of(Priority::Background), 0);
}

#[test]
fn scrolling_away_cancels_the_work_that_was_queued_for_those_cells() {
    let mut queue = PriorityQueue::default();
    let images: Vec<ImageId> = (0..5).map(|_| id()).collect();
    for image in &images {
        queue.push(*image, PixelLevel::Thumb(512), Priority::Visible);
    }
    let doomed: Vec<ImageId> = images.iter().copied().take(3).collect();
    assert_eq!(queue.cancel(&doomed), 3);
    assert_eq!(queue.len(), 2);

    // And the cancelled work can be queued again, so scrolling back works.
    assert!(queue.push(doomed[0], PixelLevel::Thumb(512), Priority::Visible));
}

#[test]
fn a_full_queue_drops_new_work_rather_than_growing() {
    let mut queue = PriorityQueue::with_capacity(2);
    assert!(queue.push(id(), PixelLevel::Proxy2048, Priority::Background));
    assert!(queue.push(id(), PixelLevel::Proxy2048, Priority::Background));
    assert!(!queue.push(id(), PixelLevel::Proxy2048, Priority::Background));
    assert_eq!(queue.len(), 2);
}

#[test]
fn a_closed_queue_accepts_nothing_and_holds_nothing() {
    let mut queue = PriorityQueue::default();
    queue.push(id(), PixelLevel::Proxy2048, Priority::Visible);
    queue.close();
    assert!(queue.is_closed());
    assert!(queue.is_empty());
    assert!(!queue.push(id(), PixelLevel::Proxy2048, Priority::Visible));
    assert!(queue.pop().is_none());
}

#[test]
fn four_thousand_batch_requests_fit_inside_the_default_capacity() {
    let mut queue = PriorityQueue::default();
    let accepted = (0..4_000)
        .filter(|_| queue.push(id(), PixelLevel::Proxy2048, Priority::AiBatch))
        .count();
    assert_eq!(accepted, 4_000);
    assert!(queue.len() <= DEFAULT_CAPACITY);
}
