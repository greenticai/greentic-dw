use crate::{ContextError, ContextPackage};

pub fn validate_context_package(package: &ContextPackage) -> Result<(), ContextError> {
    if package.package_id.trim().is_empty() {
        return Err(ContextError::Validation(
            "package_id must not be empty".to_string(),
        ));
    }
    if package.budget.max_fragments == 0 {
        return Err(ContextError::Validation(
            "budget max_fragments must be greater than zero".to_string(),
        ));
    }
    if package.budget.max_bytes == 0 {
        return Err(ContextError::Validation(
            "budget max_bytes must be greater than zero".to_string(),
        ));
    }
    if package.fragments.len() as u32 > package.budget.max_fragments {
        return Err(ContextError::Validation(
            "fragment count exceeds max_fragments budget".to_string(),
        ));
    }

    let mut previous_ordinal = None;
    for fragment in &package.fragments {
        if fragment.fragment_id.trim().is_empty()
            || fragment.content_ref.trim().is_empty()
            || fragment.provenance.trim().is_empty()
        {
            return Err(ContextError::Validation(
                "context fragments must include id, content_ref, and provenance".to_string(),
            ));
        }
        if let Some(previous) = previous_ordinal
            && fragment.ordinal < previous
        {
            return Err(ContextError::Validation(
                "context fragments must be ordered deterministically".to_string(),
            ));
        }
        previous_ordinal = Some(fragment.ordinal);
    }
    Ok(())
}
