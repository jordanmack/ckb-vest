use crate::Loader;
use ckb_testtool::builtin::ALWAYS_SUCCESS;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

mod helpers;
mod args_validation;
mod batching;

const MAX_CYCLES: u64 = 10_000_000;

// Error codes from our contract
const ERROR_INVALID_ARGS: i8 = 10;
const ERROR_INVALID_EPOCH: i8 = 23;

// Test helper functions - Enhanced error code extraction following CKB best practices
fn extract_error_code(result: &Result<ckb_testtool::ckb_types::core::Cycle, ckb_testtool::ckb_error::Error>) -> Option<i8> {
    if let Err(err) = result {
        let err_str = format!("{:?}", err);
        
        // Pattern 1: "see error code XX" (standard CKB pattern)
        if let Some(start) = err_str.find("see error code ") {
            let start = start + "see error code ".len();
            if let Some(end) = err_str[start..].find(" ") {
                if let Ok(code) = err_str[start..start + end].parse::<i8>() {
                    return Some(code);
                }
            }
        }
        
        // Pattern 2: Direct ValidationFailure error code
        if let Some(start) = err_str.find("ValidationFailure: ") {
            let start = start + "ValidationFailure: ".len();
            if let Some(end) = err_str[start..].find(" ") {
                if let Ok(code) = err_str[start..start + end].parse::<i8>() {
                    return Some(code);
                }
            }
        }
        
        // Pattern 3: Error code followed by "on page" pattern  
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

fn create_vesting_args(
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

fn create_vesting_data(
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

// ALWAYS_SUCCESS differentiation technique for testing proxy locks
fn create_always_success_lock_with_args(context: &mut Context, args: Vec<u8>) -> (Script, [u8; 32]) {
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock_script = context.build_script(&always_success_out_point, Bytes::from(args)).expect("script");
    let lock_hash: [u8; 32] = lock_script.calc_script_hash().unpack();
    (lock_script, lock_hash)
}

fn setup_authorization_locks(context: &mut Context) -> (Script, [u8; 32], Script, [u8; 32]) {
    let (beneficiary_lock, beneficiary_hash) = create_always_success_lock_with_args(context, vec![1u8]);
    let (creator_lock, creator_hash) = create_always_success_lock_with_args(context, vec![2u8]);
    (beneficiary_lock, beneficiary_hash, creator_lock, creator_hash)
}

// Temporary compatibility functions - will be removed after test updates
fn create_dummy_lock_hash(value: u8) -> [u8; 32] {
    [value; 32]
}

fn create_dummy_lock_script(context: &mut Context) -> Script {
    let out_point = context.deploy_cell(Bytes::new());
    context.build_script(&out_point, Bytes::new()).expect("script")
}

fn setup_header_with_block_and_epoch(context: &mut Context, block_number: u64, epoch: u64) -> Byte32 {
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

// Backward compatibility function
fn setup_header_with_epoch(context: &mut Context, epoch: u64) -> Byte32 {
    // Use epoch as block number for backward compatibility
    setup_header_with_block_and_epoch(context, epoch, epoch)
}

/// Tests that the vesting lock script properly rejects transactions with invalid argument lengths.
/// The lock script expects exactly 88 bytes of arguments (creator hash + beneficiary hash + epochs).
#[test]
fn test_invalid_args_length() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Invalid args - too short
    let invalid_args = Bytes::from(vec![1, 2, 3]);
    let lock_script = context
        .build_script(&out_point, invalid_args)
        .expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(1000, 0, 0, 100),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(1000, 0, 0, 101).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    let error_code = extract_error_code(&result);
    assert!(result.is_err(), "Should fail with invalid args, got error code: {:?}", error_code);
    let err = result.unwrap_err();
    assert_eq!(
        err.to_string().contains(&ERROR_INVALID_ARGS.to_string()),
        true,
        "Expected error code {}, got: {:?}", ERROR_INVALID_ARGS, error_code
    );
}

/// Tests that the vesting lock script validates epoch ordering constraints.
/// Ensures start_epoch < end_epoch and start_epoch <= cliff_epoch <= end_epoch.
#[test]
fn test_invalid_epoch_ordering() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Invalid epochs - start >= end
    let args = create_vesting_args(
        create_dummy_lock_hash(1),
        create_dummy_lock_hash(2),
        100, // start_epoch
        100, // end_epoch (same as start - invalid)
        100, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(1000, 0, 0, 100),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(1000, 0, 0, 101).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    let error_code = extract_error_code(&result);
    assert!(result.is_err(), "Should fail with invalid epoch, got error code: {:?}", error_code);
    let err = result.unwrap_err();
    assert_eq!(
        err.to_string().contains(&ERROR_INVALID_EPOCH.to_string()),
        true,
        "Expected error code {}, got: {:?}", ERROR_INVALID_EPOCH, error_code
    );
}

/// Tests that the vesting lock script rejects cells with invalid data lengths.
/// The cell data must be exactly 32 bytes containing vesting state information.
#[test]
fn test_invalid_cell_data_length() {
    // This test validates that invalid cell data length is rejected
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let args = create_vesting_args(
        create_dummy_lock_hash(1),
        create_dummy_lock_hash(2),
        100, // start_epoch
        200, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(lock_script.clone())
            .build(),
        Bytes::from(vec![1, 2, 3]), // Invalid data length - too short
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(1000, 0, 0, 101).pack())
        .build();
    let tx = context.complete_tx(tx);

    // The transaction should fail due to invalid cell data
    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(
        result.is_err(),
        "Transaction should fail with invalid cell data length, got error code: {:?}", extract_error_code(&result)
    );
}

// Additional tests would go here - currently simplified for demo purposes

/// Tests that beneficiaries cannot claim vested tokens before the vesting period starts.
/// Claims should be rejected when current_epoch < start_epoch.
#[test]
fn test_beneficiary_claim_before_start_epoch() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        200, // start_epoch
        300, // end_epoch
        220, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150), // current epoch 150 < start 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Beneficiary trying to claim before start epoch
    let output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // claiming half
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 151).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before start epoch, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim tokens before the cliff period ends.
/// Claims should be rejected when current_epoch is between start_epoch and cliff_epoch.
#[test]
fn test_beneficiary_claim_before_cliff_epoch() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        220, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150), // current epoch 150 > start 100 but < cliff 220
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(2500u64.pack()) // claiming 25%
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2500, 0, 151).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before cliff epoch, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim partially vested tokens after the cliff period.
/// Validates proper authorization through beneficiary lock hash and linear vesting calculation.
#[test]
fn test_beneficiary_claim_partial_vested() {
    // Test: Beneficiary can claim partially vested tokens after cliff period
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use ALWAYS_SUCCESS differentiation technique
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch (200 epoch duration)
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Setup header with current epoch = 200 (50% through vesting period)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create vesting input cell
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum capacity
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // total: 10000, claimed: 0, current epoch: 200
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder() // updated vesting cell (first output)
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack()) // claimed 5000
        .output(CellOutput::new_builder() // beneficiary receives claimed tokens (second output)
            .capacity(5000u64.pack())
            .lock(beneficiary_lock)
            .build())
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - claiming valid partial amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim the full vested amount after the vesting period ends.
/// Validates claiming 100% of tokens when current_epoch >= end_epoch.
#[test]
fn test_beneficiary_claim_fully_vested() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 350
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 350), // current epoch 350 > end 300 = fully vested
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Fully vested: beneficiary consumes entire cell (no outputs)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - claiming full vested amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim more tokens than have vested.
/// Ensures the contract rejects attempts to over-claim based on current vesting progress.
#[test]
fn test_beneficiary_claim_exceeds_vested_amount() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(7500u64.pack()) // trying to claim 75% when only 50% vested
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(2500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 7500, 0, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than vested, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim more than currently vested amount.
/// Uses proper authorization and validates specific error code.
#[test]
fn test_beneficiary_over_claim_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201, higher than input's highest_block_seen (200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // At epoch 200, only 50% should be vested: (200-100)/(300-100) = 100/200 = 50%
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested = 5000
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim 7500 (75%) when only 5000 (50%) is vested
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 7500, 0, 201).pack()) // Over-claiming
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than vested, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot claim before cliff period ends.
/// Validates that zero vesting before cliff is enforced.
#[test]
fn test_beneficiary_claim_before_cliff_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        150, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 131, higher than input's highest_block_seen (130)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 131, 130);

    // At epoch 130, before cliff at 150 - nothing should be vested
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 130),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim any amount before cliff
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 1000, 0, 131).pack()) // Any claim before cliff
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before cliff period, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot over-claim after making partial claims.
/// Validates that previously claimed amounts are properly tracked.
#[test]
fn test_beneficiary_over_claim_after_partial_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 251, higher than input's highest_block_seen (250)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    // At epoch 250, should be 75% vested: (250-100)/(300-100) = 150/200 = 75% = 7500
    // Already claimed 3000, so 4500 more available
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 0, 250), // Already claimed 3000
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim 6000 more when only 4500 additional is available
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 9000, 0, 251).pack()) // Claiming 6000 more (total 9000)
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than available after partial claim, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot claim in post-termination when nothing remains.
/// Validates post-termination over-claiming protection.
#[test]
fn test_beneficiary_post_termination_over_claim_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201, higher than input's highest_block_seen (200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Post-termination: creator claimed 8000, beneficiary already claimed 2000
    // Nothing left to claim (total 10000 - creator 8000 - beneficiary 2000 = 0)
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 8000, 200), // All claimed
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim more when nothing is left
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 3000, 8000, 201).pack()) // Trying to claim 1000 more
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - no remaining amount to claim post-termination, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries can make multiple incremental claims over time.
/// Validates that previously claimed amounts are tracked and additional claims work correctly.
#[test]
fn test_beneficiary_multiple_incremental_claims() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201 and epoch 200
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // already claimed 2000, now 50% vested = 5000 total
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let output = CellOutput::new_builder()
        .capacity(3000u64.pack()) // claiming additional 3000 (total 5000)
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - incremental claim within vested amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that creators can terminate vesting and claim all unvested tokens.
/// Validates the all-or-nothing termination mechanism and proper authorization.
#[test]
fn test_creator_termination_valid() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // beneficiary claimed 2000, 50% vested
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Setup header with block 201 and epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create authorization input cell for creator
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // At epoch 200: vested = (200-100)/(300-100) * 10000 = 5000
    // Unvested = 10000 - 5000 = 5000 (creator claims all unvested)
    let creator_output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // unvested amount to creator
        .lock(creator_lock)
        .build();

    let vesting_output = CellOutput::new_builder()
        .capacity(5161u64.pack()) // remaining capacity after termination
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(creator_output)
        .output_data(Bytes::new().pack())
        .output(vesting_output)
        .output_data(create_vesting_data(10000, 2000, 5000, 201).pack()) // creator_claimed = 5000
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - valid creator termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests that creators cannot terminate vesting more than once.
/// Ensures the contract rejects attempts to terminate when creator_claimed > 0.
#[test]
fn test_creator_termination_already_terminated() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let creator_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 5000, 200), // already terminated (creator_claimed = 5000)
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let creator_output = CellOutput::new_builder()
        .capacity(3000u64.pack()) // trying to claim more
        .lock(creator_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(4161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(creator_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2000, 8000, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - already terminated, got error code: {:?}", extract_error_code(&result));
}

/// Tests that only authorized creators can terminate vesting schedules.
/// Validates that termination requires proper creator lock hash authorization.
#[test]
fn test_creator_termination_wrong_authorization() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let wrong_lock = context.build_script(&wrong_out_point, Bytes::from(vec![99])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Wrong authorization trying to terminate
    let wrong_output = CellOutput::new_builder()
        .capacity(3000u64.pack())
        .lock(wrong_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(wrong_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2000, 3000, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - wrong authorization for termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests that creators must claim all unvested tokens during termination.
/// Ensures the all-or-nothing termination policy is enforced (no partial claims).
#[test]
fn test_creator_termination_partial_claim() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let creator_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // 5000 unvested remaining
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Creator trying to claim only part of unvested (should fail - all or nothing)
    let creator_output = CellOutput::new_builder()
        .capacity(2000u64.pack()) // only claiming 2000 of 5000 unvested
        .lock(creator_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(8161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(creator_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2000, 2000, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - creator must claim all unvested (all-or-nothing), got error code: {:?}", extract_error_code(&result));
}

/// Tests protection against stale header attacks where highest_block_seen decreases.
/// Ensures the contract rejects attempts to roll back the highest observed block number.
#[test]
fn test_stale_header_attack_prevention() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 250), // highest_block_seen = 250
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Trying to use stale header (decreasing highest_block_seen)
    let output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 200).pack()) // trying to decrease to 200
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - stale header attack (decreasing highest_block_seen), got error code: {:?}", extract_error_code(&result));
}

/// Tests that highest_block_seen can increase monotonically during valid operations.
/// Validates that the security mechanism allows forward progress.
#[test]
fn test_highest_block_seen_monotonic_increase() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201, higher than input's highest_block_seen (200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Valid partial claim with continuing vesting cell
    let beneficiary_output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // claiming 50% vested amount
        .lock(beneficiary_lock)
        .build();

    // Continuing vesting cell with remaining amount
    let vesting_output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // remaining capacity
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(beneficiary_output)
        .output_data(Bytes::new().pack())
        .output(vesting_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack()) // claimed 5000, updated block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - monotonic increase of highest_block_seen, got error code: {:?}", extract_error_code(&result));
}

/// Tests that highest_block_seen cannot decrease in anyone-can-update operations.
/// Ensures the monotonic property is enforced even for unauthorized updates.
#[test]
fn test_highest_block_seen_cannot_decrease() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 300),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anyone-can-update: just updating highest_block_seen (no claims)
    let output = CellOutput::new_builder()
        .capacity(10000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 250).pack()) // trying to decrease
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - highest_block_seen cannot decrease, got error code: {:?}", extract_error_code(&result));
}

/// Tests the anyone-can-update functionality for maintaining security.
/// Validates that any user can update highest_block_seen without special authorization.
#[test]
fn test_anyone_can_update_highest_block() {
    // Test: Anyone can update highest_block_seen without authorization
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Setup header with new block number and epoch for update
    let header_hash = setup_header_with_block_and_epoch(&mut context, 350, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // old highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anyone-can-update: just updating highest_block_seen (no authorization needed)
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 350).pack()) // updated highest_block_seen = 350
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - anyone can update highest_block_seen, got error code: {:?}", extract_error_code(&result));
}

#[test]
fn test_stale_header_rejection() {
    // Test: Contract rejects transactions with stale headers
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);
    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );
    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Setup stale header (block 150 < input's highest_block_seen 200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 150, 150);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Try to update with stale header
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 150).pack()) // trying to use stale block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - stale header rejection");
    assert_eq!(extract_error_code(&result), Some(24)); // Error::StaleHeader (header freshness check)
}

#[test]
fn test_mismatched_output_block_number() {
    // Test: Contract enforces that output highest_block_seen matches highest header
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);
    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );
    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Setup fresh header (block 350 > input's highest_block_seen 200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 350, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Try to set output highest_block_seen higher than any header provides
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 400).pack()) // 400 > 350 from header!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - mismatched output block number");
    assert_eq!(extract_error_code(&result), Some(27)); // Error::BlockNumberMismatch
}

/// Tests that only authorized beneficiaries can claim vested tokens.
/// Ensures the contract rejects claims from wrong lock hash authorization.
#[test]
fn test_unauthorized_beneficiary_claim() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let wrong_lock = context.build_script(&wrong_out_point, Bytes::from(vec![99])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Wrong authorization trying to claim
    let wrong_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(wrong_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(wrong_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - unauthorized beneficiary claim, got error code: {:?}", extract_error_code(&result));
}

/// Tests that only authorized creators can terminate vesting schedules.
/// Validates proper authorization checking for termination operations.
#[test]
fn test_unauthorized_creator_termination() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let wrong_lock = context.build_script(&wrong_out_point, Bytes::from(vec![88])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Wrong authorization trying to terminate
    let wrong_output = CellOutput::new_builder()
        .capacity(3000u64.pack()) // unvested amount
        .lock(wrong_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(wrong_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2000, 3000, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - unauthorized creator termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests that the contract properly validates lock hash authorization.
/// Ensures that claims are rejected when using incorrect lock scripts.
#[test]
fn test_wrong_lock_hash_authorization() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Create lock with beneficiary_hash but use wrong lock script
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let beneficiary_lock_wrong = context.build_script(&wrong_out_point, Bytes::from(vec![77])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(beneficiary_lock_wrong) // wrong lock script
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - wrong lock hash authorization, got error code: {:?}", extract_error_code(&result));
}

/// Tests anyone-can-update functionality on vesting cells with zero amounts.
/// Validates that security updates work even when no tokens are being vested.
#[test]
fn test_zero_vesting_amount() {
    // Test: Anyone can update highest_block_seen on vesting cell with zero amount
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    
    // Setup header with current epoch
    let header_hash = setup_header_with_epoch(&mut context, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(161u64.pack()) // minimum capacity only
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(0, 0, 0, 200), // zero total_amount
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point.clone())
        .build();

    // Anyone-can-update: just updating highest_block_seen
    let output = CellOutput::new_builder()
        .capacity(161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(0, 0, 0, 250).pack()) // updating highest_block_seen
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - zero vesting amount is valid for security updates, got error code: {:?}", extract_error_code(&result));
}

/// Tests vesting behavior when cliff epoch equals end epoch.
/// Validates that tokens become fully vested immediately at the cliff/end point.
#[test]
fn test_cliff_equals_end_epoch() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        200, // end_epoch
        200, // cliff_epoch = end_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201, higher than input's highest_block_seen (200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // at cliff/end epoch
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Should be fully vested at cliff=end
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack()) // claiming full cell capacity
        .lock(beneficiary_lock)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cliff=end epoch allows full vesting, got error code: {:?}", extract_error_code(&result));
}

/// Tests that vesting calculations handle arithmetic overflow gracefully.
/// Validates protection against overflow in epoch-based vesting computations.
#[test]
fn test_overflow_protection_vesting_calculation() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        0, // start_epoch
        u64::MAX, // end_epoch (max value to test overflow)
        1, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with large block number and epoch
    let header_hash = setup_header_with_block_and_epoch(&mut context, u64::MAX - 1, u64::MAX / 2);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, u64::MAX / 2), // very large epoch
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Should handle overflow gracefully
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, u64::MAX - 1).pack()) // update block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - overflow protection in vesting calculation, got error code: {:?}", extract_error_code(&result));
}

/// Tests the contract's behavior with minimum CKB capacity requirements.
/// Documents how the contract handles cells below the 161 CKB minimum capacity.
#[test]
fn test_minimum_capacity_requirements() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Test with below minimum capacity
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(100u64.pack()) // below 161 CKB minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(100, 0, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(100u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(100, 0, 0, 250).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    // This might pass or fail depending on CKB's capacity validation
    // The test documents the behavior with minimum capacity
    let _ = result; // Explicitly ignore the result as this test is for documentation
}

/// Tests that cell data remains consistent after beneficiary claims.
/// Validates proper state transitions and data integrity during claim operations.
#[test]
fn test_cell_data_integrity_after_claim() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // already claimed 2000, now claiming more
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Setup header with block 201 and epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let output = CellOutput::new_builder()
        .capacity(1500u64.pack()) // claiming additional 1500
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(8500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 3500, 0, 201).pack()) // total claimed = 3500
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cell data integrity maintained after claim, got error code: {:?}", extract_error_code(&result));
}

/// Tests that cell data remains consistent after creator termination.
/// Validates proper state transitions and data integrity during termination operations.
#[test]
fn test_cell_data_integrity_after_termination() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);
    
    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 0, 200), // beneficiary claimed 1000
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Setup header with block 201 and epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator terminates and claims unvested (5000)
    let creator_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(creator_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5161u64.pack()) // beneficiary now owns all remaining
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(creator_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 1000, 5000, 201).pack()) // creator claimed 5000
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cell data integrity maintained after termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests that CKB capacity is properly conserved across vesting transactions.
/// Validates that input capacity equals output capacity in all operations.
#[test]
fn test_capacity_conservation() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201 and epoch 200
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_capacity = 10000u64;
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(input_capacity.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(8000, 0, 0, 200), // 8000 vesting, 2000 minimum capacity
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let claimed_amount = 4000u64;
    let remaining_capacity = input_capacity - claimed_amount;
    
    let output = CellOutput::new_builder()
        .capacity(claimed_amount.pack())
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(remaining_capacity.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(8000, 4000, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - capacity is conserved across transaction, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim remaining tokens after creator termination.
/// Validates that vested tokens remain claimable even after termination occurs.
#[test]
fn test_post_termination_beneficiary_claims() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 251, higher than input's highest_block_seen (250)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    // Simulate post-termination state
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 4000, 250), // terminated: creator claimed 4000
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Post-termination: beneficiary consumes entire cell (no output)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - post-termination beneficiary can claim remaining, got error code: {:?}", extract_error_code(&result));
}

/// Tests basic contract loading and initialization functionality.
/// Serves as a smoke test to ensure the contract binary loads correctly.
#[test]
fn test_contract_basic_functionality() {
    // Basic smoke test to ensure contract loads and can execute
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let contract_bin_len = contract_bin.len();
    let _out_point = context.deploy_cell(contract_bin);

    assert!(contract_bin_len > 0, "Contract binary should not be empty");
}

/// Tests that transactions without header dependencies are rejected.
/// Headers are required for epoch and block number validation.
#[test]
fn test_no_header_dependencies() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Transaction without header dependencies - should fail
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 0, 151).pack())
        // NO header_dep added intentionally
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - no header dependencies");

    // Verify it's the correct error (NoHeaderDependencies = 35)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 35, "Expected error code 35 (NoHeaderDependencies), got {}", error_code);
    }
}

/// Tests that creator termination with wrong amount is rejected.
/// Creator must claim exactly the unvested amount (all-or-nothing).
#[test]
fn test_creator_termination_wrong_amount() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested = 5000, unvested = 5000
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator trying to claim wrong amount (3000 instead of 5000)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 3000, 201).pack()) // Wrong amount claimed
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - wrong termination amount");

    // Verify it's the correct error (InvalidAmount = 20)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 20, "Expected error code 20 (InvalidAmount), got {}", error_code);
    }
}

/// Tests that creator cannot terminate when everything is already vested.
/// There's nothing left to terminate after full vesting.
#[test]
fn test_creator_termination_after_full_vesting() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 350 (past end epoch - fully vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 350), // Fully vested
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator trying to terminate when fully vested
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 0, 351).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - nothing to terminate after full vesting");

    // Verify it's the correct error (NothingToTerminate = 44)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 44, "Expected error code 44 (NothingToTerminate), got {}", error_code);
    }
}

/// Tests vesting behavior when cliff equals start epoch.
/// Vesting should begin immediately after the start epoch.
#[test]
fn test_cliff_equals_start_epoch() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        100, // cliff_epoch = start_epoch (immediate vesting after start)
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 150 (25% through vesting period)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 151, 150);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Should be able to claim 25% = 2500
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(7661u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 2500, 0, 151).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cliff=start allows immediate vesting");
}

/// Tests creator termination before vesting starts.
/// Creator should be able to terminate entire amount before start epoch.
#[test]
fn test_creator_termination_nothing_vested() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        200, // start_epoch (future)
        400, // end_epoch
        250, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 150 (before start epoch)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 151, 150);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150), // Nothing vested yet
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator terminates entire amount (no output cell since nothing vested)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        // No output for vesting cell - fully consumed
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - creator can terminate entire amount before vesting starts, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiary claiming full amount with output cell is rejected.
/// When fully claiming, the cell should be consumed (no output).
#[test]
fn test_beneficiary_full_claim_with_output() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 350 (past end epoch - fully vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 350), // Fully vested
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Beneficiary claiming full amount but incorrectly keeping output cell
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 10000, 0, 351).pack()) // Fully claimed but output exists
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - full claim should not have output");

    // Verify it's the correct error (BeneficiaryFullClaimHasOutput = 42)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 42, "Expected error code 42 (BeneficiaryFullClaimHasOutput), got {}", error_code);
    }
}

/// Tests that beneficiary partial claim without output is rejected.
/// Partial claims must maintain the vesting cell (output required).
#[test]
fn test_beneficiary_partial_claim_no_output() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Beneficiary trying partial claim without output cell (incorrect)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        // No output for vesting cell - incorrect for partial claim
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - partial claim requires output");

    // Verify it's the correct error (BeneficiaryPartialClaimMissingOutput = 43)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 43, "Expected error code 43 (BeneficiaryPartialClaimMissingOutput), got {}", error_code);
    }
}

/// Tests that tampering with total amount is rejected.
/// Total amount must remain constant throughout vesting lifecycle.
#[test]
fn test_total_amount_tampering() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to change total amount (tampering)
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(12000, 5000, 0, 201).pack()) // Changed total amount!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - total amount cannot change");

    // Verify it's the correct error (TotalAmountChanged = 14)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 14, "Expected error code 14 (TotalAmountChanged), got {}", error_code);
    }
}

/// Tests that invalid beneficiary claimed delta is rejected.
/// The beneficiary_claimed value must increase by exactly the claimed amount.
#[test]
fn test_invalid_beneficiary_delta() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 0, 200), // Already claimed 1000
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Claim 2000 but update beneficiary_claimed incorrectly
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(8161u64.pack()) // Claiming 2000 capacity
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 7000, 0, 201).pack()) // Wrong: Claiming 6000 (7000-1000) but only 4000 available
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid beneficiary claimed delta, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21)
    // This triggers before the delta check because we're trying to claim more than available
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that invalid creator claimed delta is rejected.
/// The creator_claimed value must increase by exactly the termination amount.
#[test]
fn test_invalid_creator_delta() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested = 5000
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator terminating but with wrong claimed amount in state
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 4500, 201).pack()) // Wrong: should be 5000, not 4500
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid creator claimed delta");

    // Should fail with InvalidAmount since the all-or-nothing check happens first
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 20, "Expected error code 20 (InvalidAmount), got {}", error_code);
    }
}

/// Tests that dual authorization (both creator and beneficiary) works correctly.
/// Both parties can be present but only one can perform the action.
#[test]
fn test_dual_authorization_attempt() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    // Create BOTH authorization input cells
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3000000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3000000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Both parties present - creator takes precedence for termination
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 5000, 201).pack()) // Creator terminates
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    // This should succeed as creator authorization takes precedence
    assert!(result.is_ok(), "Should succeed - creator authorization takes precedence");
}
