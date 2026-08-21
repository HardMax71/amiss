use crate::DeliveryHeader;

pub struct AdmissionRequest<'a> {
    pub received_at_unix_millis: i64,
    pub headers: &'a [DeliveryHeader],
    pub body: &'a [u8],
}

pub struct AdmittedDelivery {
    pub route: String,
    pub source_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionRejection {
    Malformed,
    Unauthorized,
    Forbidden,
}

/// Authenticates and admits one bounded request using controller-local state.
///
/// Implementations must not perform provider network I/O. Returning
/// `Some` asserts that authentication and local plan admission bind the work's
/// route and stable source identity to the supplied raw request. `None` means
/// the authenticated request requires no work.
pub trait DeliveryAdmission: Send + Sync + 'static {
    /// # Errors
    ///
    /// Returns a bounded client rejection when authentication, request shape,
    /// or local plan admission fails.
    fn admit(
        &self,
        request: AdmissionRequest<'_>,
    ) -> Result<Option<AdmittedDelivery>, AdmissionRejection>;
}
