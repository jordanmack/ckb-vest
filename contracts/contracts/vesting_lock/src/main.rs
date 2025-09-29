#![cfg_attr(not(any(feature = "library", test)), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(any(feature = "library", test))]
extern crate alloc;

mod error;
use error::Error;

use ckb_std::{
    ckb_constants::Source,
    ckb_types::{bytes::Bytes, prelude::*},
    high_level::{
        load_cell, load_cell_data, load_cell_lock_hash, load_header, load_script,
        QueryIter,
    },
};
use core::result::Result;

#[cfg(not(any(feature = "library", test)))]
ckb_std::entry!(program_entry);
#[cfg(not(any(feature = "library", test)))]
ckb_std::default_alloc!(16384, 1258306, 64);

/// Entry point for the CKB script runtime.
/// Returns 0 for success, error code for failure.
pub fn program_entry() -> i8 {
    match main() {
        Ok(()) => 0,
        Err(err) => err as i8,
    }
}

// Lock script args structure (88 bytes total)
const CREATOR_LOCK_HASH_OFFSET: usize = 0;
const BENEFICIARY_LOCK_HASH_OFFSET: usize = 32;
const START_EPOCH_OFFSET: usize = 64;
const END_EPOCH_OFFSET: usize = 72;
const CLIFF_EPOCH_OFFSET: usize = 80;
const ARGS_LEN: usize = 88;

// Cell data structure (32 bytes total)
const TOTAL_AMOUNT_OFFSET: usize = 0;
const BENEFICIARY_CLAIMED_OFFSET: usize = 8;
const CREATOR_CLAIMED_OFFSET: usize = 16;
const HIGHEST_BLOCK_SEEN_OFFSET: usize = 24;
const DATA_LEN: usize = 32;

#[derive(Debug, Clone, Copy)]
enum AuthorizationType {
    Creator,
    Beneficiary,
    None,
}

#[derive(Debug)]
struct VestingConfig {
    creator_lock_hash: [u8; 32],
    beneficiary_lock_hash: [u8; 32],
    start_epoch: u64,
    end_epoch: u64,
    cliff_epoch: u64,
}

#[derive(Debug)]
struct VestingState {
    total_amount: u64,
    beneficiary_claimed: u64,
    creator_claimed: u64,
    highest_block_seen: u64,
}

/// Finds the input cell data that matches the current script's lock hash.
/// Used for lock scripts to locate their input cell.
fn find_matching_input_data() -> Result<Bytes, Error> {
    // Locate input cell with matching lock script hash.
    let current_script = load_script()?;
    let current_script_hash = current_script.calc_script_hash();

    let mut index = 0;
    while let Ok(input_cell) = load_cell(index, Source::Input) {
        if input_cell.lock().calc_script_hash() == current_script_hash {
            let data = load_cell_data(index, Source::Input).map_err(|_| Error::LoadCellDataFailed)?;
            return Ok(Bytes::from(data));
        }
        index += 1;
    }
    Err(Error::NoMatchingInputCell)
}

/// Finds the output cell data that matches the current script's lock hash.
/// Returns an error if no matching output cell is found.
fn find_matching_output_data() -> Result<Bytes, Error> {
    let current_script = load_script()?;
    let current_script_hash = current_script.calc_script_hash();

    let mut index = 0;
    while let Ok(output_cell) = load_cell(index, Source::Output) {
        if output_cell.lock().calc_script_hash() == current_script_hash {
            let data = load_cell_data(index, Source::Output).map_err(|_| Error::LoadCellDataFailed)?;
            return Ok(Bytes::from(data));
        }
        index += 1;
    }
    Err(Error::NoMatchingOutputCell)
}


/// Parses and validates the vesting configuration from script arguments.
/// Validates epoch ordering constraints.
fn parse_vesting_config(args: &[u8]) -> Result<VestingConfig, Error> {
    let mut creator_lock_hash = [0u8; 32];
    let mut beneficiary_lock_hash = [0u8; 32];

    creator_lock_hash
        .copy_from_slice(&args[CREATOR_LOCK_HASH_OFFSET..CREATOR_LOCK_HASH_OFFSET + 32]);
    beneficiary_lock_hash
        .copy_from_slice(&args[BENEFICIARY_LOCK_HASH_OFFSET..BENEFICIARY_LOCK_HASH_OFFSET + 32]);

    let start_epoch = u64::from_le_bytes(
        args[START_EPOCH_OFFSET..START_EPOCH_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let end_epoch = u64::from_le_bytes(
        args[END_EPOCH_OFFSET..END_EPOCH_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let cliff_epoch = u64::from_le_bytes(
        args[CLIFF_EPOCH_OFFSET..CLIFF_EPOCH_OFFSET + 8]
            .try_into()
            .unwrap(),
    );

    // Ensure epochs are in proper order: start <= cliff <= end.
    if start_epoch >= end_epoch || cliff_epoch < start_epoch || cliff_epoch > end_epoch {
        return Err(Error::InvalidEpoch);
    }

    Ok(VestingConfig {
        creator_lock_hash,
        beneficiary_lock_hash,
        start_epoch,
        end_epoch,
        cliff_epoch,
    })
}

/// Parses the vesting state from cell data.
/// Extracts amounts and block tracking information.
fn parse_vesting_state(data: &[u8]) -> Result<VestingState, Error> {
    let total_amount = u64::from_le_bytes(
        data[TOTAL_AMOUNT_OFFSET..TOTAL_AMOUNT_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let beneficiary_claimed = u64::from_le_bytes(
        data[BENEFICIARY_CLAIMED_OFFSET..BENEFICIARY_CLAIMED_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let creator_claimed = u64::from_le_bytes(
        data[CREATOR_CLAIMED_OFFSET..CREATOR_CLAIMED_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let highest_block_seen = u64::from_le_bytes(
        data[HIGHEST_BLOCK_SEEN_OFFSET..HIGHEST_BLOCK_SEEN_OFFSET + 8]
            .try_into()
            .unwrap(),
    );

    Ok(VestingState {
        total_amount,
        beneficiary_claimed,
        creator_claimed,
        highest_block_seen,
    })
}

/// Validates transaction structure based on authorization type and output presence.
/// Ensures proper input/output requirements for different operations.
fn validate_transaction_structure_with_output_flag(
    auth_type: AuthorizationType,
    input_state: &VestingState,
    output_state: &VestingState,
    has_output: bool,
) -> Result<(), Error> {
    // Count inputs with matching lock script hash.
    let current_script = load_script()?;
    let current_script_hash = current_script.calc_script_hash();

    // Count inputs with matching lock script
    let mut input_count = 0;
    let mut index = 0;
    while let Ok(input_cell) = load_cell(index, Source::Input) {
        if input_cell.lock().calc_script_hash() == current_script_hash {
            input_count += 1;
        }
        index += 1;
    }

    // Ensure exactly one input cell per vesting contract instance.
    if input_count != 1 {
        return Err(Error::MultipleInputsNotAllowed);
    }

    // Validate output requirements based on authorization and operation type.
    match auth_type {
        AuthorizationType::Creator => {
            // Creator operations always require cell continuation.
            if !has_output {
                return Err(Error::CreatorOperationMissingOutput);
            }
        }
        AuthorizationType::Beneficiary => {
            // Beneficiary can claim (with/without cell consumption) or update block.
            if has_output {
                let is_claim = output_state.beneficiary_claimed > input_state.beneficiary_claimed;
                if !is_claim {
                    // Block update only: cell continuation is valid.
                }
                // Claims with continuation are also valid.
            }
            // Cell consumption for full claims is valid.
        }
        AuthorizationType::None => {
            // Anonymous block updates require cell continuation.
            if !has_output {
                return Err(Error::AnonymousUpdateMissingOutput);
            }
        }
    }

    Ok(())
}

/// Finds the highest block number seen across all input cells.
/// Used for preventing temporal attacks with stale headers.
fn get_highest_block_from_inputs() -> Result<u64, Error> {
    let current_script = load_script()?;
    let current_script_hash = current_script.calc_script_hash();
    
    let mut highest_block = 0;
    let mut index = 0;
    
    while let Ok(input_cell) = load_cell(index, Source::Input) {
        if input_cell.lock().calc_script_hash() == current_script_hash {
            let data = load_cell_data(index, Source::Input).map_err(|_| Error::LoadCellDataFailed)?;
            if data.len() != DATA_LEN {
                return Err(Error::InputDataWrongLength);
            }
            let state = parse_vesting_state(&data)?;
            if state.highest_block_seen > highest_block {
                highest_block = state.highest_block_seen;
            }
        }
        index += 1;
    }
    
    Ok(highest_block)
}

/// Finds the highest block number from all header dependencies.
/// Used to verify header freshness.
fn get_highest_block_from_headers() -> Result<u64, Error> {
    let mut highest_block = 0;
    let mut index = 0;
    
    while let Ok(header) = load_header(index, Source::HeaderDep) {
        let block_number = header.raw().number().unpack();
        if block_number > highest_block {
            highest_block = block_number;
        }
        index += 1;
    }
    
    Ok(highest_block)
}

/// Finds the highest epoch number from all header dependencies.
/// Used for vesting calculations.
fn get_highest_epoch_from_headers() -> Result<u64, Error> {
    let mut highest_epoch = 0;
    let mut index = 0;
    
    while let Ok(header) = load_header(index, Source::HeaderDep) {
        let epoch = header.raw().epoch().unpack();
        if epoch > highest_epoch {
            highest_epoch = epoch;
        }
        index += 1;
    }
    
    Ok(highest_epoch)
}

/// Validates that at least one header dependency exists in the transaction.
/// Required for epoch and block number validation.
fn validate_headers_exist() -> Result<(), Error> {
    match load_header(0, Source::HeaderDep) {
        Ok(_) => Ok(()),
        Err(_) => Err(Error::NoHeaderDependencies), // No headers found.
    }
}

/// Validates that headers are fresher than input cells.
/// Prevents stale header attacks by ensuring headers have higher block numbers.
fn validate_header_freshness(
    highest_block_from_inputs: u64,
    highest_block_from_headers: u64,
) -> Result<(), Error> {
    if highest_block_from_headers <= highest_block_from_inputs {
        return Err(Error::StaleHeader);
    }
    Ok(())
}

/// Validates that the highest block number update is correct.
/// Ensures monotonic progression and exact matching with header data.
fn validate_highest_block_update(
    input_state: &VestingState,
    output_state: &VestingState,
    highest_block_from_headers: u64,
) -> Result<(), Error> {
    // Enforce monotonic block number progression.
    if output_state.highest_block_seen < input_state.highest_block_seen {
        return Err(Error::BlockNumberDecrease);
    }

    // Require exact match between output and header block numbers.
    if output_state.highest_block_seen != highest_block_from_headers {
        return Err(Error::BlockNumberMismatch);
    }

    Ok(())
}

/// Validates a beneficiary claim operation.
/// Checks vesting schedule, termination status, and claim amounts.
fn validate_beneficiary_claim(
    config: &VestingConfig,
    input_state: &VestingState,
    output_state: &VestingState,
    highest_epoch: u64,
) -> Result<(), Error> {
    // Calculate vested amount using current epoch.
    let vested_amount = calculate_vested_amount(
        highest_epoch,
        config.start_epoch,
        config.end_epoch,
        config.cliff_epoch,
        input_state.total_amount,
        input_state.creator_claimed,
    );

    // Determine available claim amount.
    let available_to_claim = vested_amount.saturating_sub(input_state.beneficiary_claimed);
    let claimed_amount = output_state
        .beneficiary_claimed
        .saturating_sub(input_state.beneficiary_claimed);

    // Ensure claim does not exceed vested amount.
    if claimed_amount > available_to_claim {
        return Err(Error::InsufficientVested);
    }

    // Verify state consistency after claim.
    validate_state_consistency(input_state, output_state, claimed_amount, 0)?;

    Ok(())
}

/// Validates a creator termination operation.
/// Enforces all-or-nothing unvested amount claiming.
fn validate_creator_termination(
    config: &VestingConfig,
    input_state: &VestingState,
    output_state: &VestingState,
    highest_epoch: u64,
) -> Result<(), Error> {
    // Prevent multiple terminations.
    if input_state.creator_claimed > 0 {
        return Err(Error::AlreadyTerminated);
    }

    // Calculate current vested amount for termination.
    let vested_amount = calculate_vested_amount(
        highest_epoch,
        config.start_epoch,
        config.end_epoch,
        config.cliff_epoch,
        input_state.total_amount,
        input_state.creator_claimed,
    );

    // Enforce all-or-nothing termination policy.
    let unvested_amount = input_state.total_amount.saturating_sub(vested_amount);
    let creator_claimed = output_state
        .creator_claimed
        .saturating_sub(input_state.creator_claimed);

    if creator_claimed != unvested_amount {
        return Err(Error::InvalidAmount);
    }

    // Verify state consistency after termination.
    validate_state_consistency(input_state, output_state, 0, creator_claimed)?;

    Ok(())
}

/// Validates that only the highest block number was updated.
/// Used for anyone-can-update security maintenance operations.
fn validate_block_update_only(
    input_state: &VestingState,
    output_state: &VestingState,
) -> Result<(), Error> {
    // Ensure only block tracking changed.
    if output_state.total_amount != input_state.total_amount
        || output_state.beneficiary_claimed != input_state.beneficiary_claimed
        || output_state.creator_claimed != input_state.creator_claimed
    {
        return Err(Error::InvalidStateChange);
    }

    Ok(())
}

/// Validates state transition consistency.
/// Ensures proper accounting for claim deltas and invariants.
fn validate_state_consistency(
    input_state: &VestingState,
    output_state: &VestingState,
    beneficiary_claimed_delta: u64,
    creator_claimed_delta: u64,
) -> Result<(), Error> {
    // Enforce total amount immutability.
    if output_state.total_amount != input_state.total_amount {
        return Err(Error::TotalAmountChanged);
    }

    // Verify beneficiary claim delta accuracy.
    if output_state.beneficiary_claimed
        != input_state
            .beneficiary_claimed
            .saturating_add(beneficiary_claimed_delta)
    {
        return Err(Error::InvalidBeneficiaryClaimedDelta);
    }

    // Verify creator claim delta accuracy.
    if output_state.creator_claimed
        != input_state
            .creator_claimed
            .saturating_add(creator_claimed_delta)
    {
        return Err(Error::InvalidCreatorClaimedDelta);
    }

    Ok(())
}

/// Calculates the vested amount based on epoch progression.
/// Implements linear vesting with cliff period support.
fn calculate_vested_amount(
    current_epoch: u64,
    start_epoch: u64,
    end_epoch: u64,
    cliff_epoch: u64,
    total_amount: u64,
    creator_claimed: u64,
) -> u64 {
    // Post-termination: everything not claimed by creator is vested.
    if creator_claimed > 0 {
        return total_amount.saturating_sub(creator_claimed);
    }

    // Nothing vests before start epoch.
    if current_epoch < start_epoch {
        return 0;
    }

    // Handle start >= end: instant vest at start.
    if start_epoch >= end_epoch {
        return total_amount;
    }

    // Effective cliff cannot exceed end epoch.
    let effective_cliff = cliff_epoch.min(end_epoch);
    if current_epoch < effective_cliff {
        return 0;
    }

    // Past end epoch = fully vested.
    if current_epoch >= end_epoch {
        return total_amount;
    }

    let elapsed = current_epoch - start_epoch;
    let duration = end_epoch - start_epoch;

    // Prevent overflow in vesting calculations.
    if let Some(product) = elapsed.checked_mul(total_amount) {
        product / duration
    } else {
        // Fallback to full vesting on overflow.
        total_amount
    }
}

/// Validates that script arguments have the correct length.
/// Ensures 88-byte argument structure.
fn validate_args_length(args: &Bytes) -> Result<(), Error> {
    if args.len() != ARGS_LEN {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

/// Determines authorization type using proxy lock pattern.
/// Checks input cells for creator or beneficiary authorization.
fn determine_authorization_type(vesting_config: &VestingConfig) -> Result<AuthorizationType, Error> {
    let creator_authorized = QueryIter::new(load_cell_lock_hash, Source::Input)
        .any(|lock_hash| lock_hash == vesting_config.creator_lock_hash);

    let beneficiary_authorized = QueryIter::new(load_cell_lock_hash, Source::Input)
        .any(|lock_hash| lock_hash == vesting_config.beneficiary_lock_hash);

    // Classify authorization based on input lock hashes.
    let auth_type = if creator_authorized {
        AuthorizationType::Creator
    } else if beneficiary_authorized {
        AuthorizationType::Beneficiary
    } else {
        AuthorizationType::None
    };

    Ok(auth_type)
}

/// Validates that exactly one input cell matches the current script.
/// Ensures single-cell processing for vesting contracts.
fn validate_single_input_cell() -> Result<(), Error> {
    let current_script = load_script()?;
    let current_script_hash = current_script.calc_script_hash();
    let mut input_count = 0;
    let mut index = 0;

    while let Ok(input_cell) = load_cell(index, Source::Input) {
        if input_cell.lock().calc_script_hash() == current_script_hash {
            input_count += 1;
        }
        index += 1;
    }

    if input_count != 1 {
        return Err(Error::MultipleInputsNotAllowed);
    }

    Ok(())
}

/// Loads and validates output cell data based on authorization type.
/// Returns the output state and whether an output cell exists.
fn load_output_state(
    auth_type: AuthorizationType,
    vesting_config: &VestingConfig,
    input_state: &VestingState,
    highest_epoch: u64,
) -> Result<(VestingState, bool), Error> {
    match auth_type {
        AuthorizationType::Creator | AuthorizationType::None => {
            // Creator and anonymous operations require cell continuation.
            let output_data = find_matching_output_data()?;
            if output_data.len() != DATA_LEN {
                return Err(Error::OutputDataWrongLength);
            }
            Ok((parse_vesting_state(&output_data)?, true))
        }
        AuthorizationType::Beneficiary => {
            // Beneficiary operations may continue or consume the cell.
            match find_matching_output_data() {
                Ok(output_data) => {
                    if output_data.len() != DATA_LEN {
                        return Err(Error::WrongDataLength);
                    }
                    Ok((parse_vesting_state(&output_data)?, true))
                }
                Err(_) => {
                    // Handle full cell consumption by beneficiary.
                    let vested_amount = calculate_vested_amount(
                        highest_epoch,
                        vesting_config.start_epoch,
                        vesting_config.end_epoch,
                        vesting_config.cliff_epoch,
                        input_state.total_amount,
                        input_state.creator_claimed,
                    );
                    let available_to_claim = vested_amount.saturating_sub(input_state.beneficiary_claimed);

                    // Create virtual state for consumption validation.
                    Ok((VestingState {
                        total_amount: input_state.total_amount,
                        beneficiary_claimed: input_state.beneficiary_claimed.saturating_add(available_to_claim),
                        creator_claimed: input_state.creator_claimed,
                        highest_block_seen: input_state.highest_block_seen,
                    }, false))
                }
            }
        }
    }
}

/// Validates output requirements based on authorization and vesting state.
/// Enforces proper transaction structure for different operation types.
fn validate_output_requirements(
    auth_type: AuthorizationType,
    has_output: bool,
    vested_amount: u64,
    total_amount: u64,
    creator_claimed: u64,
    beneficiary_claimed: u64,
) -> Result<(), Error> {
    match auth_type {
        AuthorizationType::Creator => {
            if vested_amount == 0 {
                // Nothing vested yet - creator terminates everything.
                if has_output {
                    return Err(Error::CreatorFullTerminationHasOutput);
                }
            } else if vested_amount < total_amount {
                // Partially vested - must continue for beneficiary.
                if !has_output {
                    return Err(Error::CreatorOperationMissingOutput);
                }
            } else {
                // Fully vested - nothing left to terminate.
                return Err(Error::NothingToTerminate);
            }
        }
        AuthorizationType::Beneficiary => {
            // In post-termination scenarios, beneficiary can claim everything not taken by creator.
            if creator_claimed > 0 {
                let remaining_amount = total_amount.saturating_sub(creator_claimed);
                let claimable_amount = remaining_amount.saturating_sub(beneficiary_claimed);

                if claimable_amount == 0 {
                    // Nothing left to claim - should not reach here with valid transaction.
                    return Err(Error::InsufficientVested);
                } else {
                    // Beneficiary can claim remaining amount and consume cell.
                    if has_output {
                        return Err(Error::BeneficiaryFullClaimHasOutput);
                    }
                }
            } else {
                // Normal vesting scenario - check based on vested amount.
                if vested_amount >= total_amount {
                    // Fully vested - must terminate cell.
                    if has_output {
                        return Err(Error::BeneficiaryFullClaimHasOutput);
                    }
                } else {
                    // Partially vested - must continue cell.
                    if !has_output {
                        return Err(Error::BeneficiaryPartialClaimMissingOutput);
                    }
                }
            }
        }
        AuthorizationType::None => {
            // Anonymous operations always require continuation.
            if !has_output {
                return Err(Error::AnonymousUpdateMissingOutput);
            }
        }
    }

    Ok(())
}

/// Main entry point for the vesting lock script.
/// Orchestrates validation of authorization, state transitions, and vesting logic.
pub fn main() -> Result<(), Error> {
    // Load and validate script arguments.
    let script = load_script()?;
    let args: Bytes = script.args().unpack();

    validate_args_length(&args)?;

    // Parse vesting configuration from arguments.
    let vesting_config = parse_vesting_config(&args)?;

    // Determine authorization type using proxy lock pattern.
    let auth_type = determine_authorization_type(&vesting_config)?;

    // Validate single input cell requirement.
    validate_single_input_cell()?;

    // Load and validate input cell state.
    let input_data = find_matching_input_data()?;
    if input_data.len() != DATA_LEN {
        return Err(Error::WrongDataLength);
    }
    let input_state = parse_vesting_state(&input_data)?;

    // Collect block and epoch data from transaction.
    let highest_block_from_inputs = get_highest_block_from_inputs()?;
    let highest_block_from_headers = get_highest_block_from_headers()?;
    let highest_epoch = get_highest_epoch_from_headers()?;

    // Validate header dependencies and freshness.
    validate_headers_exist()?;
    validate_header_freshness(highest_block_from_inputs, highest_block_from_headers)?;

    // Calculate vested amount for validation logic.
    let vested_amount = calculate_vested_amount(
        highest_epoch,
        vesting_config.start_epoch,
        vesting_config.end_epoch,
        vesting_config.cliff_epoch,
        input_state.total_amount,
        input_state.creator_claimed,
    );

    // Load and validate output cell data based on operation type.
    let (output_state, has_output) = load_output_state(
        auth_type,
        &vesting_config,
        &input_state,
        highest_epoch,
    )?;

    // Validate block number progression and consistency only when there's an actual output.
    if has_output {
        validate_highest_block_update(&input_state, &output_state, highest_block_from_headers)?;
    }

    // Validate output requirements based on authorization and vesting state.
    validate_output_requirements(
        auth_type,
        has_output,
        vested_amount,
        input_state.total_amount,
        input_state.creator_claimed,
        input_state.beneficiary_claimed,
    )?;

    // Execute authorization-specific validation logic.
    match auth_type {
        AuthorizationType::Creator => {
            // Validate creator termination operation.
            validate_creator_termination(&vesting_config, &input_state, &output_state, highest_epoch)?;
        }
        AuthorizationType::Beneficiary => {
            // Validate beneficiary claim operation.
            validate_beneficiary_claim(&vesting_config, &input_state, &output_state, highest_epoch)?;
        }
        AuthorizationType::None => {
            // Validate anonymous block update operation.
            validate_block_update_only(&input_state, &output_state)?;
        }
    }

    Ok(())
}
