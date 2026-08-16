use super::*;
use uuid::Uuid;

fn photo(value: u128) -> PhotoId {
    PhotoId::from_uuid(Uuid::from_u128(value))
}

#[test]
fn relative_rank_excludes_arbitrary_tiebreaks() {
    let first = photo(1);
    let tied = photo(2);
    let last = photo(3);
    let ranked = rank(&[(first, 0.9), (tied, 0.9), (last, 0.2)]);

    let first_rank = ranked
        .iter()
        .find_map(|(id, value)| (*id == first).then_some(*value));
    let tied_rank = ranked
        .iter()
        .find_map(|(id, value)| (*id == tied).then_some(*value));
    let last_rank = ranked
        .iter()
        .find_map(|(id, value)| (*id == last).then_some(*value));
    assert_eq!(
        first_rank, tied_rank,
        "equal scores must receive equal ranks"
    );
    assert!(first_rank.is_some_and(|value| (value - 1.0).abs() < f32::EPSILON));
    assert!(last_rank.is_some_and(|value| value.abs() < f32::EPSILON));
}
