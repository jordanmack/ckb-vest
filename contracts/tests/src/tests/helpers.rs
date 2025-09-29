use crate::Loader;
use ckb_testtool::builtin::ALWAYS_SUCCESS;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

pub const MAX_CYCLES: u64 = 10_000_000;

/// Error codes from the vesting lock contract.
pub const ERROR_INVALID_ARGS: i8 = 10;
pub const ERROR_INVALID_EPOCH: i8 = 23;

/// Extracts error codes from CKB test tool results following CKB best practices.
/// This function parses various error message formats to identify specific contract error codes.
pub fn extract_error_code(result: &Result<ckb_testtool::ckb_types::core::Cycle, ckb_testtool::ckb_error::Error>) -> Option<i8> {
    if let Err(err) = result {
        let err_str = format!("{:?}", err);

        // Pattern 1: "see error code XX" (standard CKB pattern).
        if let Some(start) = err_str.find("see error code ") {
            let start = start + "see error code ".len();
            if let Some(end) = err_str[start..].find(" ") {
                if let Ok(code) = err_str[start..start + end].parse::<i8>() {
                    return Some(code);
                }
            }
        }

        // Pattern 2: Direct ValidationFailure error code.
        if let Some(start) = err_str.find("ValidationFailure: ") {
            let start = start + "ValidationFailure: ".len();
            if let Some(end) = err_str[start..].find(" ") {
                if let Ok(code) = err_str[start..start + end].parse::<i8>() {
                    return Some(code);
                }
            }
        }

        // Pattern 3: Error code followed by "on page" pattern.
        if let Some(start) = err_str.find("error code ") {
            let start = start + "error code ".len();
            if let Some(end) = err_str[start..].find(" on page") {
                if let Ok(code) = err_str[start..start + end].parse::<i8>() {
                    return Some(code);
                }
            }
        }
    }
    None
}

/// Creates vesting lock script arguments from the given parameters.
/// The arguments are packed as 88 bytes: creator_lock_hash (32) + beneficiary_lock_hash (32) +
/// start_epoch (8) + end_epoch (8) + cliff_epoch (8).
pub fn create_vesting_args(
    creator_lock_hash: [u8; 32],
    beneficiary_lock_hash: [u8; 32],
    start_epoch: u64,
    end_epoch: u64,
    cliff_epoch: u64,
) -> Bytes {
    let mut args = Vec::with_capacity(88);
    args.extend_from_slice(&creator_lock_hash);
    args.extend_from_slice(&beneficiary_lock_hash);
    args.extend_from_slice(&start_epoch.to_le_bytes());
    args.extend_from_slice(&end_epoch.to_le_bytes());
    args.extend_from_slice(&cliff_epoch.to_le_bytes());
    Bytes::from(args)
}

/// Creates vesting cell data from the given parameters.
/// The data is packed as 32 bytes: total_amount (8) + beneficiary_claimed (8) +
/// creator_claimed (8) + highest_block_seen (8).
pub fn create_vesting_data(
    total_amount: u64,
    beneficiary_claimed: u64,
    creator_claimed: u64,
    highest_block_seen: u64,
) -> Bytes {
    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&total_amount.to_le_bytes());
    data.extend_from_slice(&beneficiary_claimed.to_le_bytes());
    data.extend_from_slice(&creator_claimed.to_le_bytes());
    data.extend_from_slice(&highest_block_seen.to_le_bytes());
    Bytes::from(data)
}

/// Creates ALWAYS_SUCCESS lock scripts with distinct arguments for testing proxy lock patterns.
/// This technique allows creating different lock scripts that all validate successfully,
/// enabling proper authorization testing in the vesting contract.
pub fn create_always_success_lock_with_args(context: &mut Context, args: Vec<u8>) -> (Script, [u8; 32]) {
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock_script = context.build_script(&always_success_out_point, Bytes::from(args)).expect("script");
    let lock_hash: [u8; 32] = lock_script.calc_script_hash().unpack();
    (lock_script, lock_hash)
}

/// Sets up authorization locks for testing beneficiary and creator operations.
/// Returns (beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) tuple.
pub fn setup_authorization_locks(context: &mut Context) -> (Script, [u8; 32], Script, [u8; 32]) {
    let (beneficiary_lock, beneficiary_hash) = create_always_success_lock_with_args(context, vec![1u8]);
    let (creator_lock, creator_hash) = create_always_success_lock_with_args(context, vec![2u8]);
    (beneficiary_lock, beneficiary_hash, creator_lock, creator_hash)
}

/// Creates a dummy lock hash for testing purposes.
/// This is a temporary compatibility function that will be removed after test updates.
pub fn create_dummy_lock_hash(value: u8) -> [u8; 32] {
    [value; 32]
}

/// Creates a dummy lock script for testing purposes.
/// This is a temporary compatibility function that will be removed after test updates.
pub fn create_dummy_lock_script(context: &mut Context) -> Script {
    let out_point = context.deploy_cell(Bytes::new());
    context.build_script(&out_point, Bytes::new()).expect("script")
}

/// Sets up a header with specific block number and epoch for testing.
/// Returns the hash of the created header that can be used as a dependency.
pub fn setup_header_with_block_and_epoch(context: &mut Context, block_number: u64, epoch: u64) -> Byte32 {
    let header = HeaderBuilder::default()
        .raw(RawHeaderBuilder::default()
            .number(block_number.pack())
            .epoch(epoch.pack())
            .build())
        .build();
    let header_view = header.into_view();
    let header_hash = header_view.hash();
    context.insert_header(header_view);
    header_hash
}

/// Sets up a header with specific epoch for backward compatibility.
/// Uses the epoch value as both block number and epoch for simplicity.
pub fn setup_header_with_epoch(context: &mut Context, epoch: u64) -> Byte32 {
    // Use epoch as block number for backward compatibility.
    setup_header_with_block_and_epoch(context, epoch, epoch)
}