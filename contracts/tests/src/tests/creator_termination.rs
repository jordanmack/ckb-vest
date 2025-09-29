use super::helpers::*;

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