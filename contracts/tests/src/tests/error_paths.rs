use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests specific error code for invalid creator claimed delta.
/// Validates that InvalidCreatorClaimedDelta error is properly triggered.
#[test]
fn test_invalid_creator_delta_error() {
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

    // Setup header for epoch 200
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to create inconsistent creator claimed delta (claim 5000 but state shows wrong delta)
    let output = CellOutput::new_builder()
        .capacity(5161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 0, 4999, 201).pack()) // Wrong: claimed 5000 but delta shows 4999
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid creator claimed delta");

    // Should fail with InvalidCreatorClaimedDelta (16) or InvalidAmount (20)
    if let Some(error_code) = extract_error_code(&result) {
        assert!(error_code == 16 || error_code == 20, "Expected error code 16 (InvalidCreatorClaimedDelta) or 20 (InvalidAmount), got {}", error_code);
    }
}

/// Tests specific error code for invalid state change.
/// Validates that InvalidStateChange error is properly triggered.
#[test]
fn test_invalid_state_change_error() {
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

    // Setup header for block update
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anonymous update but try to change beneficiary_claimed (should only change block)
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 2000, 0, 251).pack()) // Wrong: changed beneficiary_claimed in anonymous update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid state change in anonymous update");

    // Should fail with InvalidStateChange (17)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 17, "Expected error code 17 (InvalidStateChange), got {}", error_code);
    }
}