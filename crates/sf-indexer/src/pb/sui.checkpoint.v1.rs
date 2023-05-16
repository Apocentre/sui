#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CheckpointData {
    #[prost(message, optional, tag = "1")]
    pub checkpoint: ::core::option::Option<Checkpoint>,
    #[prost(message, repeated, tag = "2")]
    pub transactions: ::prost::alloc::vec::Vec<Transaction>,
    #[prost(message, repeated, tag = "3")]
    pub changed_objects: ::prost::alloc::vec::Vec<ChangedObject>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Checkpoint {
    /// Checkpoint's epoch ID
    #[prost(uint64, tag = "1")]
    pub epoch: u64,
    /// Checkpoint sequence number
    #[prost(uint64, tag = "2")]
    pub sequence_number: u64,
    /// Checkpoint digest (base58 encoded)
    #[prost(bytes = "vec", tag = "3")]
    pub digest: ::prost::alloc::vec::Vec<u8>,
    /// Total number of transactions committed since genesis, including those in this checkpoint.
    #[prost(uint64, tag = "4")]
    pub network_total_transactions: u64,
    /// Digest of the previous checkpoint
    #[prost(bytes = "vec", optional, tag = "5")]
    pub previous_digest: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
    /// The running total gas costs of all transactions included in the current epoch so far until this checkpoint.
    #[prost(message, optional, tag = "6")]
    pub epoch_rolling_gas_cost_summary: ::core::option::Option<GasCostSummary>,
    /// Timestamp of the checkpoint - number of milliseconds from the Unix epoch
    /// Checkpoint timestamps are monotonic, but not strongly monotonic - subsequent
    /// checkpoints can have same timestamp if they originate from the same underlining consensus commit
    #[prost(uint64, tag = "7")]
    pub timestamp_ms: u64,
    /// Present only on the final checkpoint of the epoch.
    #[prost(message, optional, tag = "8")]
    pub end_of_epoch_data: ::core::option::Option<EndOfEpochData>,
    /// Transaction digests (base58 encoded)
    #[prost(bytes = "vec", repeated, tag = "9")]
    pub transactions: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    /// Commitments to checkpoint state
    #[prost(message, repeated, tag = "10")]
    pub checkpoint_commitments: ::prost::alloc::vec::Vec<CheckpointCommitment>,
    /// Validator Signature (base64  encoded)
    #[prost(bytes = "vec", tag = "11")]
    pub validator_signature: ::prost::alloc::vec::Vec<u8>,
}
/// TODO
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {}
/// TODO
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChangedObject {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GasCostSummary {
    /// Cost of computation/execution
    #[prost(uint64, tag = "1")]
    pub computation_cost: u64,
    /// Storage cost, it's the sum of all storage cost for all objects created or mutated.
    #[prost(uint64, tag = "2")]
    pub storage_cost: u64,
    /// The amount of storage cost refunded to the user for all objects deleted or mutated in the transaction.
    #[prost(uint64, tag = "3")]
    pub storage_rebate: u64,
    /// The fee for the rebate. The portion of the storage rebate kept by the system.
    #[prost(uint64, tag = "4")]
    pub non_refundable_storage_fee: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EndOfEpochData {
    /// next_epoch_committee is `Some` if and only if the current checkpoint is
    /// the last checkpoint of an epoch.
    /// Therefore next_epoch_committee can be used to pick the last checkpoint of an epoch,
    /// which is often useful to get epoch level summary stats like total gas cost of an epoch,
    /// or the total number of transactions from genesis to the end of an epoch.
    /// The committee is stored as a vector of validator pub key and stake pairs. The vector
    /// should be sorted based on the Committee data structure.
    #[prost(message, repeated, tag = "1")]
    pub next_epoch_committee: ::prost::alloc::vec::Vec<NextEpochCommittee>,
    /// The protocol version that is in effect during the epoch that starts immediately after this checkpoint.
    #[prost(uint64, tag = "2")]
    pub next_epoch_protocol_version: u64,
    /// Commitments to epoch specific state (e.g. live object set)
    #[prost(message, repeated, tag = "3")]
    pub epoch_commitments: ::prost::alloc::vec::Vec<CheckpointCommitment>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NextEpochCommittee {
    #[prost(bytes = "vec", tag = "1")]
    pub authority_name: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "2")]
    pub stake_unit: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CheckpointCommitment {
    #[prost(oneof = "checkpoint_commitment::CheckpointCommitment", tags = "1")]
    pub checkpoint_commitment: ::core::option::Option<
        checkpoint_commitment::CheckpointCommitment,
    >,
}
/// Nested message and enum types in `CheckpointCommitment`.
pub mod checkpoint_commitment {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum CheckpointCommitment {
        #[prost(message, tag = "1")]
        EcmhLiveObjectSetDigest(super::EcmhLiveObjectSetDigest),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EcmhLiveObjectSetDigest {
    /// base58 encoded
    #[prost(bytes = "vec", tag = "1")]
    pub digest: ::prost::alloc::vec::Vec<u8>,
}
