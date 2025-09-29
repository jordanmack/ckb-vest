use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests that batched beneficiary claims are rejected.
/// Validates that multiple vesting inputs in one transaction are not allowed.
#[test]
fn test_batched_beneficiary_claims_rejected() {
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

    // Create two vesting input cells with the same lock script
    let vesting_input1_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(5000, 0, 0, 200),
    );

    let vesting_input2_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(3000, 0, 0, 200),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to create a batched transaction with two vesting inputs
    let output = CellOutput::new_builder()
        .capacity(5161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input1_out_point).build())
        .input(CellInput::new_builder().previous_output(vesting_input2_out_point).build()) // Second vesting input
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(5000, 1000, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - batched operations not allowed, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (MultipleInputsNotAllowed = 36)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 36, "Expected error code 36 (MultipleInputsNotAllowed), got {}", error_code);
    }
}

/// Tests that batched creator terminations are rejected.
/// Validates that creators cannot terminate multiple vesting contracts in one transaction.
#[test]
fn test_batched_creator_terminations_rejected() {
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

    // Setup header with block 151, higher than input's highest_block_seen (150)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 151, 150);

    // Create two vesting input cells
    let vesting_input1_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(4161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(4000, 0, 0, 150),
    );

    let vesting_input2_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(6000, 0, 0, 150),
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to create outputs for partial terminations
    let output1 = CellOutput::new_builder()
        .capacity(4161u64.pack())
        .lock(lock_script.clone())
        .build();

    let output2 = CellOutput::new_builder()
        .capacity(6161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input1_out_point).build())
        .input(CellInput::new_builder().previous_output(vesting_input2_out_point).build()) // Second vesting input
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(output1)
        .output_data(create_vesting_data(4000, 0, 1000, 151).pack()) // Partial termination
        .output(output2)
        .output_data(create_vesting_data(6000, 0, 1500, 151).pack()) // Partial termination
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - batched creator operations not allowed, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (MultipleInputsNotAllowed = 36)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 36, "Expected error code 36 (MultipleInputsNotAllowed), got {}", error_code);
    }
}

/// Tests that batched anonymous updates are rejected.
/// Validates that multiple anonymous block updates cannot be batched.
#[test]
fn test_batched_anonymous_updates_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

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

    // Create two vesting input cells
    let vesting_input1_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(7000, 1000, 0, 250),
    );

    let vesting_input2_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(5000, 500, 0, 250),
    );

    // No authorization cells - this should be anonymous update

    // Create outputs with only block number updates
    let output1 = CellOutput::new_builder()
        .capacity(7161u64.pack())
        .lock(lock_script.clone())
        .build();

    let output2 = CellOutput::new_builder()
        .capacity(5161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input1_out_point).build())
        .input(CellInput::new_builder().previous_output(vesting_input2_out_point).build()) // Second vesting input
        .output(output1)
        .output_data(create_vesting_data(7000, 1000, 0, 251).pack()) // Only block update
        .output(output2)
        .output_data(create_vesting_data(5000, 500, 0, 251).pack()) // Only block update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - batched anonymous updates not allowed, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (MultipleInputsNotAllowed = 36)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 36, "Expected error code 36 (MultipleInputsNotAllowed), got {}", error_code);
    }
}

/// Tests that mixed operations with different vesting contracts are allowed.
/// Validates that different vesting contracts (different args) can run in the same transaction.
#[test]
fn test_mixed_different_contracts_allowed() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    // Create two different vesting configurations
    let args1 = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let args2 = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        200, // start_epoch - different
        400, // end_epoch - different
        220, // cliff_epoch - different
    );

    let lock_script1 = context.build_script(&out_point, args1).expect("script1");
    let lock_script2 = context.build_script(&out_point, args2).expect("script2");

    // Setup header with block 251, higher than input's highest_block_seen (250)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    // Create vesting input cells with different lock scripts
    let vesting_input1_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script1.clone())
            .build(),
        create_vesting_data(5000, 0, 0, 250),
    );

    let vesting_input2_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3161u64.pack())
            .lock(lock_script2.clone())
            .build(),
        create_vesting_data(3000, 0, 0, 250),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Create outputs for both contracts
    let output1 = CellOutput::new_builder()
        .capacity(5161u64.pack())
        .lock(lock_script1.clone())
        .build();

    let output2 = CellOutput::new_builder()
        .capacity(3161u64.pack())
        .lock(lock_script2.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input1_out_point).build())
        .input(CellInput::new_builder().previous_output(vesting_input2_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output1)
        .output_data(create_vesting_data(5000, 1000, 0, 251).pack())
        .output(output2)
        .output_data(create_vesting_data(3000, 600, 0, 251).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - different vesting contracts can run in same transaction, got error code: {:?}", extract_error_code(&result));
}

/// Tests that multiple inputs with identical vesting contracts are rejected.
/// Validates that multiple cells with the same lock script args cannot be batched.
#[test]
fn test_identical_contracts_batching_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    // Create identical vesting configuration for both cells
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

    // Create two vesting input cells with IDENTICAL lock scripts (same args)
    let vesting_input1_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(5000, 0, 0, 200),
    );

    let vesting_input2_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3161u64.pack())
            .lock(lock_script.clone()) // SAME lock script
            .build(),
        create_vesting_data(3000, 0, 0, 200),
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to create outputs for both (this should fail)
    let output1 = CellOutput::new_builder()
        .capacity(5161u64.pack())
        .lock(lock_script.clone())
        .build();

    let output2 = CellOutput::new_builder()
        .capacity(3161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input1_out_point).build())
        .input(CellInput::new_builder().previous_output(vesting_input2_out_point).build()) // Second identical input
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output1)
        .output_data(create_vesting_data(5000, 1000, 0, 201).pack())
        .output(output2)
        .output_data(create_vesting_data(3000, 600, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - identical vesting contracts cannot be batched, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (MultipleInputsNotAllowed = 36)
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 36, "Expected error code 36 (MultipleInputsNotAllowed), got {}", error_code);
    }
}