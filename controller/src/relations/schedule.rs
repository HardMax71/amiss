use crate::{LeaseFence, RelationTransition, relation_transition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRelation {
    pub transition: RelationTransition,
    pub fence: LeaseFence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelationAdmission {
    Scheduled(PendingRelation),
    Duplicate(PendingRelation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationScheduleError {
    #[error("the relation transition is invalid")]
    InvalidTransition,
    #[error("the relation identity is bound to different operator configuration")]
    BindingConflict,
    #[error("the coordination identity is bound to different subject revisions")]
    CoordinationConflict,
    #[error("the relation scheduling generation is exhausted")]
    GenerationExhausted,
}

/// Admits exact relation work without deriving coordination or ordering from
/// timestamps. Identical work preserves its first pending value; a new
/// coordination identity advances the fence and supersedes the old worker.
///
/// # Errors
///
/// The previous or requested transition is invalid, one stable identity is
/// rebound, or the fence cannot advance.
pub fn schedule_relation(
    previous: Option<PendingRelation>,
    transition: RelationTransition,
) -> Result<RelationAdmission, RelationScheduleError> {
    let transition = relation_transition(
        transition.relation,
        transition.coordination,
        transition.subjects,
    )
    .map_err(|_defect| RelationScheduleError::InvalidTransition)?;
    let Some(previous) = previous else {
        let fence = LeaseFence::new(1).ok_or(RelationScheduleError::GenerationExhausted)?;
        return Ok(RelationAdmission::Scheduled(PendingRelation {
            transition,
            fence,
        }));
    };
    let checked_previous = relation_transition(
        previous.transition.relation.clone(),
        previous.transition.coordination.clone(),
        previous.transition.subjects.clone(),
    )
    .map_err(|_defect| RelationScheduleError::InvalidTransition)?;
    if checked_previous.relation.plan != transition.relation.plan {
        return Err(RelationScheduleError::BindingConflict);
    }
    if checked_previous.coordination == transition.coordination {
        return if checked_previous.subjects == transition.subjects {
            Ok(RelationAdmission::Duplicate(previous))
        } else {
            Err(RelationScheduleError::CoordinationConflict)
        };
    }
    let fence = previous
        .fence
        .get()
        .checked_add(1)
        .and_then(LeaseFence::new)
        .ok_or(RelationScheduleError::GenerationExhausted)?;
    Ok(RelationAdmission::Scheduled(PendingRelation {
        transition,
        fence,
    }))
}
