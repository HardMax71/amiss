use amiss_controller_gitea::{repository::RepositoryRecord, user::UserRecord};
use js_int::{MAX_SAFE_INT, MAX_SAFE_UINT};

pub(super) fn assert_integer_contract<T>(
    record: &T,
    bound: i64,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert!(bound == MAX_SAFE_INT || bound == -MAX_SAFE_INT);
    let encoded = serde_json::to_string(record)?;
    assert_eq!(&serde_json::from_str::<T>(&encoded)?, record);
    let marker = bound.to_string();
    let outside = bound
        .checked_add(bound.signum())
        .ok_or("invalid numeric boundary")?
        .to_string();
    let offsets: Vec<_> = encoded
        .match_indices(&marker)
        .map(|(offset, _)| offset)
        .collect();
    assert!(!offsets.is_empty());
    for offset in offsets {
        let end = offset
            .checked_add(marker.len())
            .ok_or("invalid numeric span")?;
        for invalid in [
            outside.as_str(),
            "-0",
            "1.0",
            "1e0",
            "\"1\"",
            "null",
            "true",
            "[]",
            "{}",
        ] {
            let mut changed = encoded.clone();
            changed.replace_range(offset..end, invalid);
            assert!(
                serde_json::from_str::<T>(&changed).is_err(),
                "offset {offset}: {invalid}"
            );
        }
    }
    Ok(())
}

#[test]
fn user_identity_keeps_the_safe_integer_boundary() -> Result<(), Box<dyn std::error::Error>> {
    for input in [
        include_bytes!("../fixtures/gitea-user.json").as_slice(),
        include_bytes!("../fixtures/forgejo-user.json").as_slice(),
    ] {
        let mut user: UserRecord = serde_json::from_slice(input)?;
        user.id = MAX_SAFE_UINT;
        assert_integer_contract(&user, MAX_SAFE_INT)?;
        user.id = MAX_SAFE_UINT + 1;
        assert!(serde_json::to_vec(&user).is_err());
        user.id = 0;
        assert_eq!(
            serde_json::from_slice::<UserRecord>(&serde_json::to_vec(&user)?)?,
            user
        );
    }
    Ok(())
}

#[test]
fn repository_and_owner_identities_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut repository: RepositoryRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-fork.json"))?;
    repository.id = MAX_SAFE_UINT;
    repository.owner.id = MAX_SAFE_UINT;
    assert_integer_contract(&repository, MAX_SAFE_INT)?;
    for oversized in [
        RepositoryRecord {
            id: MAX_SAFE_UINT + 1,
            ..repository.clone()
        },
        RepositoryRecord {
            owner: UserRecord {
                id: MAX_SAFE_UINT + 1,
                ..repository.owner.clone()
            },
            ..repository
        },
    ] {
        assert!(serde_json::to_vec(&oversized).is_err());
    }
    Ok(())
}
