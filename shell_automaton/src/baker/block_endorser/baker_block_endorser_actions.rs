// Copyright (c) SimpleStaking, Viable Systems and Tezedge Contributors
// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

use crypto::hash::{BlockPayloadHash, Signature};
use tezos_messages::base::signature_public_key::SignaturePublicKeyHash;
use tezos_messages::p2p::encoding::block_header::Level;

use crate::baker::BakerState;
use crate::current_head::CurrentHeadState;
use crate::request::RequestId;
use crate::rights::EndorsingPower;
use crate::{EnablingCondition, State};

use super::{BakerBlockEndorserState, EndorsementWithForgedBytes, PreendorsementWithForgedBytes};

fn current_head_level_round_payload(state: &State) -> Option<(Level, i32, &BlockPayloadHash)> {
    match &state.current_head {
        CurrentHeadState::Rehydrated {
            head, payload_hash, ..
        } => {
            let round = head.header.fitness().round()?;
            Some((head.header.level(), round, payload_hash.as_ref()?))
        }
        _ => None,
    }
}

fn is_payload_outdated(state: &State, baker: &BakerState) -> Option<bool> {
    let (level, round, _) = current_head_level_round_payload(state)?;
    let locked_payload = match baker.locked_payload.as_ref() {
        Some(v) => v,
        None => return Some(false),
    };

    Some(
        level < locked_payload.level()
            || (level == locked_payload.level() && round < locked_payload.round()),
    )
}

fn should_preendorse(state: &State, baker: &BakerState) -> Option<bool> {
    let (level, _, payload_hash) = current_head_level_round_payload(state)?;
    let locked_payload = match baker.locked_payload.as_ref() {
        Some(v) => v,
        None => return Some(true),
    };

    let can_accept_payload = locked_payload.level() < level
        || locked_payload.payload_hash().eq(payload_hash)
        || state.mempool.prequorum.is_reached();

    is_payload_outdated(state, baker).map(|v| !v && can_accept_payload)
}

fn should_start(state: &State, baker: &BakerState) -> bool {
    fn _should_start(state: &State, baker: &BakerState) -> Option<bool> {
        if !baker.persisted.is_rehydrated() {
            return Some(false);
        }
        match &baker.block_endorser {
            BakerBlockEndorserState::Idle { .. }
            | BakerBlockEndorserState::EndorsementInjectSuccess { .. } => {}
            _ => return Some(false),
        }
        let persisted = baker.persisted.current_state()?;
        let head = state.current_head.get()?;
        let last_endorsement = match persisted.last_endorsement {
            Some(v) => v.operation.operation(),
            None => return Some(true),
        };

        let head_level = head.header.level();
        let head_round = head.header.fitness().round()?;
        Some(
            last_endorsement.level < head_level
                || (last_endorsement.level == head_level && last_endorsement.round < head_round),
        )
    }
    _should_start(state, baker).unwrap_or(false)
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserRightsGetInitAction {}

impl EnablingCondition<State> for BakerBlockEndorserRightsGetInitAction {
    fn is_enabled(&self, state: &State) -> bool {
        let is_ready = state.bakers.iter().any(|(_, b)| should_start(state, b));
        is_ready && state.is_bootstrapped()
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserRightsGetPendingAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserRightsGetPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        state
            .bakers
            .get(&self.baker)
            .map_or(false, |baker| should_start(state, baker))
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserRightsGetSuccessAction {
    pub baker: SignaturePublicKeyHash,
    pub first_slot: u16,
    pub endorsing_power: EndorsingPower,
}

impl EnablingCondition<State> for BakerBlockEndorserRightsGetSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::RightsGetPending { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserRightsNoRightsAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserRightsNoRightsAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::RightsGetPending { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPayloadOutdatedAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPayloadOutdatedAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::RightsGetSuccess { .. }
            ) && is_payload_outdated(state, baker).unwrap_or(false)
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPayloadLockedAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPayloadLockedAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::RightsGetSuccess { .. }
            ) && !is_payload_outdated(state, baker).unwrap_or(false)
                && !should_preendorse(state, baker).unwrap_or(false)
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPayloadUnlockedAsPreQuorumReachedAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPayloadUnlockedAsPreQuorumReachedAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PayloadLocked { .. }
            ) && should_preendorse(state, baker).unwrap_or(false)
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorseAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorseAction {
    fn is_enabled(&self, state: &State) -> bool {
        state
            .bakers
            .get(&self.baker)
            .map_or(false, |baker| match &baker.block_endorser {
                BakerBlockEndorserState::RightsGetSuccess { .. }
                | BakerBlockEndorserState::PayloadUnlockedAsPreQuorumReached { .. } => {
                    should_preendorse(state, baker).unwrap_or(false)
                }
                _ => false,
            })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorsementSignInitAction {
    pub baker: SignaturePublicKeyHash,
    pub operation: PreendorsementWithForgedBytes,
}

impl BakerBlockEndorserPreendorsementSignInitAction {
    fn should_sign(
        state: &State,
        baker: &SignaturePublicKeyHash,
        operation: &PreendorsementWithForgedBytes,
    ) -> bool {
        let baker_state = match state.bakers.get(baker) {
            Some(v) => v,
            None => return false,
        };
        let preendorsement = operation.operation();
        match &baker_state.block_endorser {
            BakerBlockEndorserState::Preendorse { first_slot, .. } => {
                preendorsement.slot == *first_slot
                    && state
                        .current_head
                        .level()
                        .map_or(false, |level| preendorsement.level == level)
                    && state
                        .current_head
                        .round()
                        .map_or(false, |round| preendorsement.round == round)
                    && state
                        .current_head
                        .payload_hash()
                        .map_or(false, |p_hash| &preendorsement.block_payload_hash == p_hash)
            }
            _ => false,
        }
    }
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorsementSignInitAction {
    fn is_enabled(&self, state: &State) -> bool {
        BakerBlockEndorserPreendorsementSignInitAction::should_sign(
            state,
            &self.baker,
            &self.operation,
        )
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorsementSignPendingAction {
    pub baker: SignaturePublicKeyHash,
    pub operation: PreendorsementWithForgedBytes,
    pub req_id: RequestId,
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorsementSignPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        BakerBlockEndorserPreendorsementSignInitAction::should_sign(
            state,
            &self.baker,
            &self.operation,
        )
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorsementSignSuccessAction {
    pub baker: SignaturePublicKeyHash,
    pub signature: Signature,
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorsementSignSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PreendorsementSignPending { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorsementInjectPendingAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorsementInjectPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PreendorsementSignSuccess { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPreendorsementInjectSuccessAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPreendorsementInjectSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PreendorsementInjectPending { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPrequorumPendingAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPrequorumPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PreendorsementInjectSuccess { .. }
            ) && !state.mempool.prequorum.is_reached()
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserPrequorumSuccessAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserPrequorumSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::PreQuorumPending { .. }
            ) && state.mempool.prequorum.is_reached()
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorseAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserEndorseAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.mempool.prequorum.is_reached()
            && state
                .bakers
                .get(&self.baker)
                .map_or(false, |baker| match &baker.block_endorser {
                    BakerBlockEndorserState::PreQuorumSuccess { .. } => true,
                    BakerBlockEndorserState::PreendorsementInjectSuccess { .. } => true,
                    _ => false,
                })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorsementSignInitAction {
    pub baker: SignaturePublicKeyHash,
    pub operation: EndorsementWithForgedBytes,
}

impl BakerBlockEndorserEndorsementSignInitAction {
    fn should_sign(
        state: &State,
        baker: &SignaturePublicKeyHash,
        operation: &EndorsementWithForgedBytes,
    ) -> bool {
        let baker_state = match state.bakers.get(baker) {
            Some(v) => v,
            None => return false,
        };
        let endorsement = operation.operation();

        matches!(
            &baker_state.block_endorser,
            BakerBlockEndorserState::Endorse { .. }
        ) && baker_state
            .block_endorser
            .first_slot()
            .map_or(false, |slot| slot == endorsement.slot)
            && state
                .current_head
                .level()
                .map_or(false, |level| endorsement.level == level)
            && state
                .current_head
                .round()
                .map_or(false, |round| endorsement.round == round)
            && state
                .current_head
                .payload_hash()
                .map_or(false, |p_hash| &endorsement.block_payload_hash == p_hash)
    }
}

impl EnablingCondition<State> for BakerBlockEndorserEndorsementSignInitAction {
    fn is_enabled(&self, state: &State) -> bool {
        BakerBlockEndorserEndorsementSignInitAction::should_sign(
            state,
            &self.baker,
            &self.operation,
        )
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorsementSignPendingAction {
    pub baker: SignaturePublicKeyHash,
    pub operation: EndorsementWithForgedBytes,
    pub req_id: RequestId,
}

impl EnablingCondition<State> for BakerBlockEndorserEndorsementSignPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        BakerBlockEndorserEndorsementSignInitAction::should_sign(
            state,
            &self.baker,
            &self.operation,
        )
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorsementSignSuccessAction {
    pub baker: SignaturePublicKeyHash,
    pub signature: Signature,
}

impl EnablingCondition<State> for BakerBlockEndorserEndorsementSignSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::EndorsementSignPending { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserStatePersistPendingAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserStatePersistPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::EndorsementSignSuccess { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserStatePersistSuccessAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserStatePersistSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state
            .bakers
            .get(&self.baker)
            .map_or(false, |baker| match &baker.block_endorser {
                BakerBlockEndorserState::StatePersistPending { state_counter, .. } => {
                    baker.persisted.last_persisted_counter() >= *state_counter
                }
                _ => false,
            })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorsementInjectPendingAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserEndorsementInjectPendingAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::StatePersistSuccess { .. }
            )
        })
    }
}

#[cfg_attr(feature = "fuzzing", derive(fuzzcheck::DefaultMutator))]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BakerBlockEndorserEndorsementInjectSuccessAction {
    pub baker: SignaturePublicKeyHash,
}

impl EnablingCondition<State> for BakerBlockEndorserEndorsementInjectSuccessAction {
    fn is_enabled(&self, state: &State) -> bool {
        state.bakers.get(&self.baker).map_or(false, |baker| {
            matches!(
                baker.block_endorser,
                BakerBlockEndorserState::EndorsementInjectPending { .. }
            )
        })
    }
}
