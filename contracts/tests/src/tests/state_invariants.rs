use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests that beneficiary_claimed cannot decrease between transactions.
/// Ensures monotonic progression of beneficiary claims to prevent rollback attacks.
#[test]
fn test_beneficiary_claimed_cannot_decrease() {
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

    // Setup header with block 201, higher than input's highest_block_seen (200).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 0, 200), // Already claimed 3000
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to decrease beneficiary_claimed from 3000 to 2000.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 2000, 0, 201).pack()) // Decreasing claimed amount!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - beneficiary_claimed cannot decrease");

    // Verify it's the correct error (InvalidBeneficiaryClaimedDelta = 15).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 15, "Expected error code 15 (InvalidBeneficiaryClaimedDelta), got {}", error_code);
    }
}

/// Tests that creator_claimed cannot decrease between transactions.
/// Ensures that termination operations are irreversible.
#[test]
fn test_creator_claimed_cannot_decrease() {
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

    // Setup header with block 201, higher than input's highest_block_seen (200).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 5000, 200), // Creator already claimed 5000
    );

    // Create creator authorization input cell.
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to decrease creator_claimed from 5000 to 3000.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(7161u64.pack()) // Capacity would increase if claim decreased
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 1000, 3000, 201).pack()) // Decreasing creator_claimed!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - creator_claimed cannot decrease");

    // Verify it's an appropriate error (AlreadyTerminated = 22 or InvalidCreatorClaimedDelta = 16).
    if let Some(error_code) = extract_error_code(&result) {
        assert!(error_code == 22 || error_code == 16,
            "Expected error code 22 (AlreadyTerminated) or 16 (InvalidCreatorClaimedDelta), got {}", error_code);
    }
}

/// Tests that both beneficiary_claimed and creator_claimed cannot decrease in the same transaction.
/// Validates that all claim fields maintain monotonic progression.
#[test]
fn test_both_claims_cannot_decrease() {
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

    // Setup header for anonymous update.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 2000, 200), // Both have claimed something
    );

    // Try to decrease both claims (anonymous update attempting rollback).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 2500, 1500, 251).pack()) // Both decreased!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claims cannot decrease");

    // Will fail with InvalidStateChange since anonymous update changed amounts.
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 17, "Expected error code 17 (InvalidStateChange), got {}", error_code);
    }
}

/// Tests that beneficiary_claimed can stay the same (no change is valid).
/// Validates that claims don't need to increase every transaction.
#[test]
fn test_beneficiary_claimed_can_stay_same() {
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

    // Setup header for anonymous update.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 0, 200), // beneficiary_claimed = 3000
    );

    // Keep beneficiary_claimed the same (just update block).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 3000, 0, 251).pack()) // Same claimed amount
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - beneficiary_claimed can stay the same");
}

/// Tests that creator_claimed can stay the same in post-termination state.
/// Validates that terminated contracts can be updated without changing claim amounts.
#[test]
fn test_creator_claimed_can_stay_same() {
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

    // Setup header for anonymous update.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 5000, 200), // Post-termination: creator_claimed = 5000
    );

    // Keep creator_claimed the same (just update block).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(6161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 1000, 5000, 251).pack()) // Same creator_claimed
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - creator_claimed can stay the same in post-termination");
}
