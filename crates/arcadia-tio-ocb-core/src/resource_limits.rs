#![allow(dead_code)]

//! Finite resource limits for untrusted OCB reads.
//!
//! The selected-compressed limit is applied with independent counters to an
//! owned read's selected compressed chunks and to the unique encoded auxiliary
//! objects (dictionary values and key tuples) encountered during one open.

use crate::{ArcadiaTioError, Result};

pub(crate) const OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES: u64 = 1_073_741_824;
pub(crate) const OCB_POLICY_A_MAX_COMPRESSED_CHUNK_BYTES: u64 = 536_870_912;
pub(crate) const OCB_POLICY_A_MAX_DECOMPRESSED_CHUNK_BYTES: u64 = 536_870_912;
pub(crate) const OCB_POLICY_A_MAX_PROJECTED_ROW_GROUP_BYTES: u64 = 1_073_741_824;
pub(crate) const OCB_POLICY_A_MAX_OWNED_SELECTED_COMPRESSED_BYTES: u64 = 8_589_934_592;
pub(crate) const OCB_POLICY_A_MAX_OWNED_DECODED_MATERIALIZED_BYTES: u64 = 17_179_869_184;

/// Finite byte limits applied consistently throughout one opened OCB handle.
///
/// Policy A is the compatibility default. Larger finite limits can be selected
/// explicitly for reviewed workloads; visitor and streaming APIs remain the
/// preferred path for data sets that exceed the default owned-result totals.
/// `max_owned_selected_compressed_bytes` independently bounds both an owned
/// request's selected compressed chunks and the aggregate unique encoded
/// auxiliary objects (dictionary values and key tuples) encountered during one
/// open. Those two uses have separate counters; consuming one does not reduce
/// the other.
/// `max_owned_decoded_materialized_bytes` independently bounds both an owned
/// request's decoded/materialized result and the aggregate logical heap
/// requested while materializing and validating one root candidate. V1 has one
/// candidate; V2 can try at most two candidates sequentially, and a rejected
/// candidate is dropped before the next attempt. Those two uses have separate
/// counters; consuming one does not reduce the other.
/// These are logical and accounting bounds, not a peak resident-memory promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcbResourceLimits {
    max_encoded_object_bytes: u64,
    max_compressed_chunk_bytes: u64,
    max_decompressed_chunk_bytes: u64,
    max_projected_row_group_bytes: u64,
    max_owned_selected_compressed_bytes: u64,
    max_owned_decoded_materialized_bytes: u64,
}

impl OcbResourceLimits {
    /// Construct a finite resource policy.
    ///
    /// Zero is valid and is useful for fail-closed callers and bounded tests.
    /// Limits that directly size one allocation must fit the platform's
    /// addressable allocation domain. Aggregate owned-result totals are `u64`
    /// accounting limits and therefore may exceed that domain.
    pub fn new(
        max_encoded_object_bytes: u64,
        max_compressed_chunk_bytes: u64,
        max_decompressed_chunk_bytes: u64,
        max_projected_row_group_bytes: u64,
        max_owned_selected_compressed_bytes: u64,
        max_owned_decoded_materialized_bytes: u64,
    ) -> Result<Self> {
        validate_direct_allocation_limit(max_encoded_object_bytes)?;
        validate_direct_allocation_limit(max_compressed_chunk_bytes)?;
        validate_direct_allocation_limit(max_decompressed_chunk_bytes)?;
        validate_direct_allocation_limit(max_projected_row_group_bytes)?;
        Ok(Self {
            max_encoded_object_bytes,
            max_compressed_chunk_bytes,
            max_decompressed_chunk_bytes,
            max_projected_row_group_bytes,
            max_owned_selected_compressed_bytes,
            max_owned_decoded_materialized_bytes,
        })
    }

    /// Return the approved compatibility default resource policy.
    pub const fn policy_a() -> Self {
        Self {
            max_encoded_object_bytes: OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES,
            max_compressed_chunk_bytes: OCB_POLICY_A_MAX_COMPRESSED_CHUNK_BYTES,
            max_decompressed_chunk_bytes: OCB_POLICY_A_MAX_DECOMPRESSED_CHUNK_BYTES,
            max_projected_row_group_bytes: OCB_POLICY_A_MAX_PROJECTED_ROW_GROUP_BYTES,
            max_owned_selected_compressed_bytes: OCB_POLICY_A_MAX_OWNED_SELECTED_COMPRESSED_BYTES,
            max_owned_decoded_materialized_bytes: OCB_POLICY_A_MAX_OWNED_DECODED_MATERIALIZED_BYTES,
        }
    }

    pub const fn max_encoded_object_bytes(self) -> u64 {
        self.max_encoded_object_bytes
    }

    pub const fn max_compressed_chunk_bytes(self) -> u64 {
        self.max_compressed_chunk_bytes
    }

    pub const fn max_decompressed_chunk_bytes(self) -> u64 {
        self.max_decompressed_chunk_bytes
    }

    pub const fn max_projected_row_group_bytes(self) -> u64 {
        self.max_projected_row_group_bytes
    }

    /// Return the independent owned-read and open-auxiliary aggregate limit.
    pub const fn max_owned_selected_compressed_bytes(self) -> u64 {
        self.max_owned_selected_compressed_bytes
    }

    /// Return the independent owned-result and root-candidate metadata limit.
    pub const fn max_owned_decoded_materialized_bytes(self) -> u64 {
        self.max_owned_decoded_materialized_bytes
    }

    pub fn with_max_encoded_object_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            value,
            self.max_compressed_chunk_bytes,
            self.max_decompressed_chunk_bytes,
            self.max_projected_row_group_bytes,
            self.max_owned_selected_compressed_bytes,
            self.max_owned_decoded_materialized_bytes,
        )
    }

    pub fn with_max_compressed_chunk_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            self.max_encoded_object_bytes,
            value,
            self.max_decompressed_chunk_bytes,
            self.max_projected_row_group_bytes,
            self.max_owned_selected_compressed_bytes,
            self.max_owned_decoded_materialized_bytes,
        )
    }

    pub fn with_max_decompressed_chunk_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            self.max_encoded_object_bytes,
            self.max_compressed_chunk_bytes,
            value,
            self.max_projected_row_group_bytes,
            self.max_owned_selected_compressed_bytes,
            self.max_owned_decoded_materialized_bytes,
        )
    }

    pub fn with_max_projected_row_group_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            self.max_encoded_object_bytes,
            self.max_compressed_chunk_bytes,
            self.max_decompressed_chunk_bytes,
            value,
            self.max_owned_selected_compressed_bytes,
            self.max_owned_decoded_materialized_bytes,
        )
    }

    pub fn with_max_owned_selected_compressed_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            self.max_encoded_object_bytes,
            self.max_compressed_chunk_bytes,
            self.max_decompressed_chunk_bytes,
            self.max_projected_row_group_bytes,
            value,
            self.max_owned_decoded_materialized_bytes,
        )
    }

    pub fn with_max_owned_decoded_materialized_bytes(self, value: u64) -> Result<Self> {
        Self::new(
            self.max_encoded_object_bytes,
            self.max_compressed_chunk_bytes,
            self.max_decompressed_chunk_bytes,
            self.max_projected_row_group_bytes,
            self.max_owned_selected_compressed_bytes,
            value,
        )
    }
}

impl Default for OcbResourceLimits {
    fn default() -> Self {
        Self::policy_a()
    }
}

/// Monotonic accounting for heap requested by materialized metadata objects.
///
/// One instance is shared across one V1 open or one V2 root-candidate attempt.
/// Parsers charge it only after completely validating an encoded object's
/// structure, but before any retained vector/string reserve or payload copy.
/// A rejected V2 candidate and its allocations are dropped before a fresh
/// budget is used for the next candidate. The encoded input buffer and
/// temporary stack values are not charged. A caller may construct a separate
/// instance for any later operation that materializes metadata independently.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MetadataMaterializationBudget {
    limit_bytes: u64,
    charged_bytes: u64,
}

impl MetadataMaterializationBudget {
    pub(crate) const fn new(limit_bytes: u64) -> Self {
        Self {
            limit_bytes,
            charged_bytes: 0,
        }
    }

    pub(crate) const fn from_limits(limits: OcbResourceLimits) -> Self {
        Self::new(limits.max_owned_decoded_materialized_bytes())
    }

    pub(crate) const fn limit_bytes(&self) -> u64 {
        self.limit_bytes
    }

    pub(crate) const fn charged_bytes(&self) -> u64 {
        self.charged_bytes
    }

    pub(crate) const fn remaining_bytes(&self) -> u64 {
        self.limit_bytes - self.charged_bytes
    }

    pub(crate) fn charge(&mut self, bytes: u64) -> Result<()> {
        let Some(charged_bytes) = self.charged_bytes.checked_add(bytes) else {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB open metadata materialization exceeds resource limit",
            ));
        };
        if charged_bytes > self.limit_bytes {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB open metadata materialization exceeds resource limit",
            ));
        }
        self.charged_bytes = charged_bytes;
        Ok(())
    }
}

fn validate_direct_allocation_limit(value: u64) -> Result<()> {
    if value > isize::MAX as u64 {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB direct-allocation resource limit exceeds the platform addressable size",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OcbFailureCause;

    #[test]
    fn policy_a_values_and_custom_limits_are_stable() {
        let policy_a = OcbResourceLimits::policy_a();
        assert_eq!(policy_a, OcbResourceLimits::default());
        assert_eq!(policy_a.max_encoded_object_bytes(), 1_073_741_824);
        assert_eq!(policy_a.max_compressed_chunk_bytes(), 536_870_912);
        assert_eq!(policy_a.max_decompressed_chunk_bytes(), 536_870_912);
        assert_eq!(policy_a.max_projected_row_group_bytes(), 1_073_741_824);
        assert_eq!(
            policy_a.max_owned_selected_compressed_bytes(),
            8_589_934_592
        );
        assert_eq!(
            policy_a.max_owned_decoded_materialized_bytes(),
            17_179_869_184
        );

        let zero = OcbResourceLimits::new(0, 0, 0, 0, 0, 0).expect("zero limits");
        assert_eq!(zero.max_encoded_object_bytes(), 0);
        assert_eq!(zero.max_owned_decoded_materialized_bytes(), 0);
    }

    #[test]
    fn direct_allocation_limits_must_be_platform_addressable() {
        let oversized = (isize::MAX as u64).checked_add(1).unwrap();
        let err = OcbResourceLimits::policy_a()
            .with_max_encoded_object_bytes(oversized)
            .unwrap_err();
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));

        let aggregate = OcbResourceLimits::policy_a()
            .with_max_owned_decoded_materialized_bytes(u64::MAX)
            .expect("aggregate accounting limit");
        assert_eq!(aggregate.max_owned_decoded_materialized_bytes(), u64::MAX);
    }

    #[test]
    fn metadata_materialization_budget_is_exact_and_monotonic() {
        let limits = OcbResourceLimits::policy_a()
            .with_max_owned_decoded_materialized_bytes(7)
            .expect("aggregate limit");
        let mut budget = MetadataMaterializationBudget::from_limits(limits);
        assert_eq!(budget.limit_bytes(), 7);
        assert_eq!(budget.remaining_bytes(), 7);

        budget.charge(3).expect("first allocation");
        budget.charge(4).expect("exact aggregate limit");
        assert_eq!(budget.charged_bytes(), 7);
        assert_eq!(budget.remaining_bytes(), 0);

        let err = budget.charge(1).expect_err("exact-minus-one must fail");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
        assert_eq!(budget.charged_bytes(), 7);

        let mut overflow = MetadataMaterializationBudget::new(u64::MAX);
        overflow.charge(u64::MAX).expect("maximum exact charge");
        let err = overflow
            .charge(1)
            .expect_err("aggregate overflow must fail");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
        assert_eq!(overflow.charged_bytes(), u64::MAX);
    }
}
