use crate::{ContextError, context_package_fixture, validate_context_package};

#[test]
fn validates_context_fixture() {
    validate_context_package(&context_package_fixture()).expect("fixture should validate");
}

#[test]
fn rejects_out_of_order_fragments() {
    let mut package = context_package_fixture();
    package.fragments.swap(0, 1);
    let err = validate_context_package(&package).expect_err("out-of-order fragments should fail");
    assert_eq!(
        err,
        ContextError::Validation("context fragments must be ordered deterministically".to_string())
    );
}
