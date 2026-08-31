mod tests;

use amiss_controller::{IntegrationId, ProviderError, RelationSubject, RelationSubjectHead};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

use super::Client;
use super::model::CommitRecord;

pub(super) trait GitHubRelationRest {
    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
    ) -> Result<CommitRecord, ProviderError>;
}

impl<R: GitHubRelationRest> Client<R> {
    pub(super) fn resolve_relation_head(
        &self,
        subject: &RelationSubject,
    ) -> Result<RelationSubjectHead, ProviderError> {
        let expected_integration = IntegrationId::new(self.config.installation_id.to_string())
            .ok_or(ProviderError::InvalidResponse)?;
        let repository = &subject.scope.repository;
        if subject.scope.provider != self.config.provider
            || subject.scope.integration != expected_integration
            || repository.host() != self.config.provider.instance.as_str()
            || !crate::acquisition::canonical_github_repository(repository)
            || subject.object_format != ObjectFormat::Sha1
        {
            return Err(ProviderError::InvalidResponse);
        }

        let head = self.rest.relation_head(repository, &subject.target)?;
        let candidate_commit =
            Oid::new(ObjectFormat::Sha1, head.sha).ok_or(ProviderError::InvalidResponse)?;
        Oid::new(ObjectFormat::Sha1, head.tree).ok_or(ProviderError::InvalidResponse)?;
        Ok(RelationSubjectHead {
            subject: subject.clone(),
            candidate_commit,
        })
    }
}
