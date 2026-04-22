use crate::{ReflectionError, ReviewOutcome, review_outcome_fixture};

#[test]
fn validates_review_fixture() {
    review_outcome_fixture()
        .validate()
        .expect("fixture should validate");
}

#[test]
fn rejects_invalid_score() {
    let err = ReviewOutcome {
        score: Some(1.5),
        ..review_outcome_fixture()
    }
    .validate()
    .expect_err("score above one should fail");
    assert_eq!(
        err,
        ReflectionError::Validation("review score must be between 0.0 and 1.0".to_string())
    );
}
