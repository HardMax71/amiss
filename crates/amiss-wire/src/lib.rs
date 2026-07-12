#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitClass {
    /// Complete evaluation, no effective blocking finding.
    Success,
    /// Complete evaluation, at least one effective blocking finding.
    BlockingFindings,
    /// Anything that prevented a trustworthy complete result.
    Failure,
}

impl ExitClass {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::BlockingFindings => 1,
            Self::Failure => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExitClass;

    #[test]
    fn exit_codes_are_contract() {
        assert_eq!(ExitClass::Success.code(), 0);
        assert_eq!(ExitClass::BlockingFindings.code(), 1);
        assert_eq!(ExitClass::Failure.code(), 2);
    }
}
