// Copyright (c) SimpleStaking, Viable Systems and Tezedge Contributors
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crypto::hash::{
    BlockMetadataHash, BlockPayloadHash, ChainId, NonceHash, OperationMetadataListListHash,
    Signature,
};
use storage::BlockHeaderWithHash;
use tezos_encoding::enc::{BinError, BinWriter};
use tezos_encoding::encoding::HasEncoding;
use tezos_encoding::types::SizedBytes;
use tezos_messages::p2p::encoding::block_header::BlockHeader;
use tezos_messages::p2p::encoding::operation::Operation;
use tezos_messages::p2p::encoding::operations_for_blocks::Path;
use tezos_messages::protocol::SupportedProtocol;
use tezos_messages::Timestamp;

use crate::protocol_runner::ProtocolRunnerToken;
use crate::request::RequestId;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BakingSlot {
    pub round: u32,
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BakerBlockBakerState {
    Idle {
        time: u64,
    },
    RightsGetPending {
        time: u64,
        /// Slots for current level.
        slots: Option<Vec<u16>>,
        /// Slots for next level.
        next_slots: Option<Vec<u16>>,
    },
    RightsGetSuccess {
        time: u64,
        /// Slots for current level.
        slots: Vec<u16>,
        /// Slots for next level.
        next_slots: Vec<u16>,
    },
    NoRights {
        time: u64,
    },
    /// Waiting until current level/round times out and until it's time
    /// for us to bake a block.
    TimeoutPending {
        time: u64,
        /// Slot for current level's next round that we can bake.
        next_round: Option<BakingSlot>,
        /// Slots for next level's next round that we can bake.
        next_level: Option<BakingSlot>,

        /// Has timeout happened or not. This will only be true if
        /// timeout did happen, but we have no elected_block
        /// (block which reached endorsement quorum).
        next_level_timeout_notified: bool,
    },
    /// Previous round didn't reach the quorum, or we aren't baker of
    /// the next level and we haven't seen next level block yet, so
    /// it's time to bake next round.
    BakeNextRound {
        time: u64,
        round: u32,
        block_timestamp: Timestamp,
    },
    /// Previous round did reach the quorum, so bake the next level.
    BakeNextLevel {
        time: u64,
        round: u32,
        block_timestamp: Timestamp,
    },
    BuildBlock {
        time: u64,
        block: BuiltBlock,
    },
    PreapplyPending {
        time: u64,
        protocol_req_id: ProtocolRunnerToken,
        request: BlockPreapplyRequest,
    },
    PreapplySuccess {
        time: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    ComputeProofOfWorkPending {
        time: u64,
        req_id: RequestId,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    ComputeProofOfWorkSuccess {
        time: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    SignPending {
        time: u64,
        req_id: RequestId,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    SignSuccess {
        time: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    StatePersistPending {
        time: u64,
        state_counter: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    StatePersistSuccess {
        time: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    ComputeOperationsPathsPending {
        time: u64,
        protocol_req_id: ProtocolRunnerToken,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
    },
    ComputeOperationsPathsSuccess {
        time: u64,
        header: BlockHeader,
        operations: Vec<Vec<Operation>>,
        operations_paths: Vec<Path>,
    },
    InjectPending {
        time: u64,
        block: BlockHeaderWithHash,
        operations: Vec<Vec<Operation>>,
        operations_paths: Vec<Path>,
    },
    InjectSuccess {
        time: u64,
        block: BlockHeaderWithHash,
    },
}

impl BakerBlockBakerState {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle { .. })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum LiquidityBakingToggleVote {
    On,
    Off,
    Pass,
}

impl LiquidityBakingToggleVote {
    /// Encode for 012-PsIthaca protocol.
    pub fn bin_write_v012(self) -> u8 {
        match self {
            Self::On => 0xff,
            // `Pass` option wasn't there for ithaca.
            Self::Off | Self::Pass => 0x00,
        }
    }

    /// Encode for 013-PtJakarta protocol.
    pub fn bin_write_v013(self) -> u8 {
        match self {
            Self::On => 0x00,
            Self::Off => 0x01,
            Self::Pass => 0x02,
        }
    }

    pub fn bin_write_for_protocol(self, protocol: SupportedProtocol) -> u8 {
        match protocol {
            SupportedProtocol::Proto001 => 0x00,
            SupportedProtocol::Proto002 => 0x00,
            SupportedProtocol::Proto003 => 0x00,
            SupportedProtocol::Proto004 => 0x00,
            SupportedProtocol::Proto005 => 0x00,
            SupportedProtocol::Proto005_2 => 0x00,
            SupportedProtocol::Proto006 => 0x00,
            SupportedProtocol::Proto007 => 0x00,
            SupportedProtocol::Proto008 => 0x00,
            SupportedProtocol::Proto008_2 => 0x00,
            SupportedProtocol::Proto009 => 0x00,
            SupportedProtocol::Proto010 => 0x00,
            SupportedProtocol::Proto011 => 0x00,
            // ^ unsupported protocols.
            SupportedProtocol::Proto012 => self.bin_write_v012(),
            SupportedProtocol::Proto013 => self.bin_write_v013(),
        }
    }
}

impl Default for LiquidityBakingToggleVote {
    fn default() -> Self {
        LiquidityBakingToggleVote::Off
    }
}

impl FromStr for LiquidityBakingToggleVote {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on" => Ok(LiquidityBakingToggleVote::On),
            "off" => Ok(LiquidityBakingToggleVote::Off),
            "pass" => Ok(LiquidityBakingToggleVote::Pass),
            _ => Err("unknown value".to_string()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuiltBlock {
    pub round: i32,
    pub payload_round: i32,
    pub timestamp: Timestamp,
    pub payload_hash: BlockPayloadHash,
    pub proof_of_work_nonce: SizedBytes<8>,
    pub seed_nonce_hash: Option<NonceHash>,
    pub liquidity_baking_escape_vote: LiquidityBakingToggleVote,
    pub operations: Vec<Vec<Operation>>,

    pub predecessor_header: BlockHeader,
    pub predecessor_max_operations_ttl: i32,
    pub pred_block_metadata_hash: BlockMetadataHash,
    pub pred_ops_metadata_hash: Option<OperationMetadataListListHash>,
}

impl BuiltBlock {
    pub fn bin_encode_protocol_data(
        &self,
        protocol: SupportedProtocol,
    ) -> Result<Vec<u8>, BinError> {
        #[derive(BinWriter, HasEncoding, Serialize)]
        pub struct ProtocolData {
            pub payload_hash: BlockPayloadHash,
            pub payload_round: i32,
            pub proof_of_work_nonce: SizedBytes<8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub seed_nonce_hash: Option<NonceHash>,
            pub liquidity_baking_escape_vote: u8,
            pub signature: Signature,
        }

        // PERF(zura): Extra Cloning.
        let data = ProtocolData {
            payload_hash: self.payload_hash.clone(),
            payload_round: self.payload_round,
            proof_of_work_nonce: self.proof_of_work_nonce.clone(),
            seed_nonce_hash: self.seed_nonce_hash.clone(),
            liquidity_baking_escape_vote: self
                .liquidity_baking_escape_vote
                .bin_write_for_protocol(protocol),
            signature: Signature(vec![0; 64]),
        };

        let mut v = vec![];
        data.bin_write(&mut v)?;
        Ok(v)
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockPreapplyRequest {
    pub chain_id: ChainId,
    pub protocol_data: Vec<u8>,
    pub timestamp: Timestamp,
    pub operations: Vec<Vec<Operation>>,
    pub predecessor_header: BlockHeader,
    pub predecessor_block_metadata_hash: BlockMetadataHash,
    pub predecessor_ops_metadata_hash: Option<OperationMetadataListListHash>,
    pub predecessor_max_operations_ttl: i32,
}

impl From<BlockPreapplyRequest> for tezos_api::ffi::PreapplyBlockRequest {
    fn from(req: BlockPreapplyRequest) -> Self {
        tezos_api::ffi::PreapplyBlockRequest {
            chain_id: req.chain_id.clone(),
            protocol_data: req.protocol_data.clone(),
            timestamp: Some(req.timestamp.i64()),
            operations: req.operations.clone(),
            predecessor_header: req.predecessor_header.clone(),
            predecessor_block_metadata_hash: Some(req.predecessor_block_metadata_hash.clone()),
            predecessor_ops_metadata_hash: req.predecessor_ops_metadata_hash.clone(),
            predecessor_max_operations_ttl: req.predecessor_max_operations_ttl,
        }
    }
}

pub type BlockPreapplyResponse = tezos_api::ffi::PreapplyBlockResponse;
