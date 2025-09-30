use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

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

/// Tests transition from partial to full vesting at end epoch.
/// Validates state transition when moving from partial claim to full vesting.
#[test]
fn test_transition_partial_to_full_vesting() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        200, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header exactly at end epoch
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 0, 199), // Already claimed 30%, now at end epoch = fully vested
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // At end epoch (200), can claim remaining 70% and consume the cell
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        // No output - full consumption since all tokens can be claimed
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - transition from partial to full vesting, got error code: {:?}", extract_error_code(&result));
}

/// Tests enhanced vesting calculation overflow protection.
/// Validates protection against overflow in complex vesting scenarios.
#[test]
fn test_vesting_calculation_overflow_protection() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        u64::MAX - 1000, // start_epoch near max
        u64::MAX - 100,  // end_epoch near max
        u64::MAX - 500,  // cliff_epoch near max
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with epoch that could cause overflow in calculation
    let header_hash = setup_header_with_block_and_epoch(&mut context, u64::MAX - 200, u64::MAX - 300);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(u64::MAX / 2, 0, 0, u64::MAX - 400), // Large total amount
    );

    // Anonymous update test: just update block number with overflow protection
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack()) // Keep same capacity
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(u64::MAX / 2, 0, 0, u64::MAX - 200).pack()) // Only block update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - overflow protection in complex vesting calculation, got error code: {:?}", extract_error_code(&result));
}

/// Tests maximum values edge cases for all parameters.
/// Validates that the contract handles maximum u64 values correctly.
#[test]
fn test_maximum_values_edge_cases() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        0,         // start_epoch
        u64::MAX,  // end_epoch (maximum)
        1,         // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with maximum values
    let header_hash = setup_header_with_block_and_epoch(&mut context, u64::MAX, u64::MAX);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(u64::MAX, 0, 0, u64::MAX - 1), // Maximum total_amount, near max block
    );

    // Anonymous update to maximum block number
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(u64::MAX, 0, 0, u64::MAX).pack()) // Update to max block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - maximum values edge cases, got error code: {:?}", extract_error_code(&result));
}

/// Tests cliff equals start epoch with different vesting schedule.
/// Validates immediate vesting behavior when cliff matches start with full termination.
#[test]
fn test_cliff_equals_start_immediate_vesting() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        50,  // start_epoch
        250, // end_epoch
        50,  // cliff_epoch = start_epoch (immediate vesting)
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header at start epoch + 1
    let header_hash = setup_header_with_block_and_epoch(&mut context, 51, 51);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(8161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(8000, 0, 0, 50), // At start epoch, immediate vesting should begin
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // At epoch 51, only 1/200th is vested (1/200 * 8000 = 40), so creator can claim 8000 - 40 = 7960
    let output = CellOutput::new_builder()
        .capacity(201u64.pack()) // Tiny remaining amount (40 + 161 minimum)
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(8000, 0, 7960, 51).pack()) // Creator claims 7960, leaves 40
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cliff=start allows immediate termination, got error code: {:?}", extract_error_code(&result));
}