pub use amiss_controller_fixtures::clock::TestClock;
use std::time::{SystemTime, UNIX_EPOCH};

use amiss_controller::{ProviderIdentity, ProviderInstance, ProviderNamespace};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

pub const HOST: &str = "gitlab.example";
pub const PROJECT_PATH: &str = "acme/widget";

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new(HOST.to_owned()).unwrap(),
    }
}

pub fn repository() -> RepositoryIdentity {
    RepositoryIdentity::new(HOST.to_owned(), "acme".to_owned(), "widget".to_owned()).unwrap()
}

pub fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

pub fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}
