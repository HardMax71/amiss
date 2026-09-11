use amiss_controller_gitea::{repository::RepositoryRecord, user::UserRecord};
use amiss_wire::assessment::Nullable;
use js_int::{MAX_SAFE_INT, MAX_SAFE_UINT, UInt};

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
    assert_eq!(
        &amiss_wire::read_json::<T>(encoded.as_bytes(), u64::MAX)?,
        record
    );
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
            assert!(amiss_wire::read_json::<T>(changed.as_bytes(), u64::MAX).is_err());
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
fn nested_repository_counts_and_optional_branches_are_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    let mut repository: RepositoryRecord =
        serde_json::from_slice(include_bytes!("../fixtures/gitea-fork.json"))?;
    let mut pending = vec![&mut repository];
    while let Some(repository) = pending.pop() {
        for count in [
            &mut repository.id,
            &mut repository.size,
            &mut repository.stars_count,
            &mut repository.forks_count,
            &mut repository.watchers_count,
            &mut repository.open_issues_count,
            &mut repository.open_pr_counter,
            &mut repository.release_counter,
            &mut repository.owner.id,
        ] {
            *count = MAX_SAFE_UINT;
        }
        repository.branch_count = Some(UInt::MAX);
        if let Some(Nullable::Value(parent)) = &mut repository.parent {
            pending.push(parent);
        }
    }
    assert_integer_contract(&repository, MAX_SAFE_INT)?;
    for count in [None, Some(UInt::MIN), Some(UInt::MAX)] {
        repository.branch_count = count;
        let encoded = serde_json::to_vec(&repository)?;
        assert_eq!(
            amiss_wire::read_json::<RepositoryRecord>(&encoded, u64::MAX)?,
            repository
        );
    }
    repository.size = MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&repository).is_err());
    Ok(())
}
