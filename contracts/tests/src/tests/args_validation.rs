use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

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