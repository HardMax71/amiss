use std::sync::LazyLock;

use amiss_controller_gitea::webhook::{HookIssueAction, PullRequestPayload};
use amiss_fixtures::GITEA_PULL_WEBHOOK;
use serde::Serialize;

use crate::{WebhookExercise, number, selection, text};

mod tests;

#[expect(
    clippy::expect_used,
    reason = "the shared typed fixture must remain valid"
)]
static FIXTURE: LazyLock<(PullRequestPayload, String)> = LazyLock::new(|| {
    let payload: PullRequestPayload = amiss_wire::read_json(GITEA_PULL_WEBHOOK, u64::MAX)
        .expect("the shared webhook fixture is complete");
    let body = serde_json::to_string(&payload).expect("the typed webhook serializes");
    (payload, body)
});

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum PullField<'a> {
    Action(&'a str),
    Number(u64),
    Id(u64),
    Sha(&'a str),
    Ref(&'a str),
    Name(&'a str),
}

#[expect(
    clippy::expect_used,
    reason = "wire corruption must reach the intended fixture field"
)]
pub(super) fn prepare_webhook(data: &[u8]) -> WebhookExercise<'_> {
    let (payload, wire) = &*FIXTURE;
    let pull = payload
        .pull_request
        .as_ref()
        .expect("the fixture has a pull");
    let repository = payload
        .repository
        .as_ref()
        .expect("the fixture has a repository");
    let head = pull.head.sha.as_ref().expect("the fixture has a head ID");
    let action = payload.action.to_string();
    let synchronized = HookIssueAction::Synchronized.to_string();
    let mutation = data.get(1..).unwrap_or_default();
    let string = text(mutation);
    let integer = number(mutation);
    let choices = [
        (
            PullField::Action(&action),
            PullField::Action(&synchronized),
            0,
        ),
        (PullField::Action(&action), PullField::Action(&string), 0),
        (
            PullField::Number(payload.number),
            PullField::Number(integer),
            0,
        ),
        (
            PullField::Number(pull.number),
            PullField::Number(integer),
            1,
        ),
        (PullField::Id(pull.id), PullField::Id(integer), 0),
        (PullField::Sha(head.as_str()), PullField::Sha(&string), 0),
        (
            PullField::Ref(&pull.head.branch),
            PullField::Ref(&string),
            0,
        ),
        (
            PullField::Ref(&pull.base.branch),
            PullField::Ref(&string),
            0,
        ),
        (
            PullField::Name(&repository.name),
            PullField::Name(&string),
            0,
        ),
    ];
    let selector = selection(data, 10);
    let mut body = wire.clone();
    if let Some((original, replacement, occurrence)) =
        selector.checked_sub(1).and_then(|index| choices.get(index))
    {
        // Invalid scalars enter only after the complete typed payload reaches its wire boundary.
        let original = serde_json::to_string(original).expect("the original field serializes");
        let replacement =
            serde_json::to_string(replacement).expect("the replacement field serializes");
        let needle = original
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .expect("Serde wraps the tagged field in an object");
        let replacement = replacement
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .expect("Serde wraps the tagged field in an object");
        let (offset, _) = body
            .match_indices(needle)
            .nth(*occurrence)
            .expect("the selected field occurs at its fixture position");
        let end = offset
            .checked_add(needle.len())
            .expect("the matched field is inside the body");
        body.replace_range(offset..end, replacement);
    }
    WebhookExercise {
        body: body.into_bytes(),
        data,
        target_matches: selector != 8 || string == "main",
        family: "Gitea-family",
    }
}
