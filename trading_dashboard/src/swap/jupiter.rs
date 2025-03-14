


// swap with jupiter
async fn swap(
    &self,
    state: &SinglePosition,
    amount_in: u64,
    swap_for_y: bool,
    is_simulation: bool,
) -> Result<SwapEvent> {
    // let state = self.get_state();
    let lb_pair_state = state.lb_pair_state;
    let lb_pair = state.lb_pair;
    let active_bin_array_idx = BinArray::bin_id_to_bin_array_index(lb_pair_state.active_id)?;

    let payer = read_keypair_file(self.wallet.clone().unwrap())
        .map_err(|_| Error::msg("Requires a keypair file"))?;
    let program: Program<Arc<Keypair>> = create_program(
        self.provider.to_string(),
        self.provider.to_string(),
        lb_clmm::ID,
        Arc::new(Keypair::new()),
    )?;
    let (bin_array_0, _bump) = derive_bin_array_pda(lb_pair, active_bin_array_idx as i64);

    let (user_token_in, user_token_out, bin_array_1, bin_array_2) = if swap_for_y {
        (
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint),
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint),
            derive_bin_array_pda(lb_pair, (active_bin_array_idx - 1) as i64).0,
            derive_bin_array_pda(lb_pair, (active_bin_array_idx - 2) as i64).0,
        )
    } else {
        (
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_y_mint),
            get_associated_token_address(&payer.pubkey(), &lb_pair_state.token_x_mint),
            derive_bin_array_pda(lb_pair, (active_bin_array_idx + 1) as i64).0,
            derive_bin_array_pda(lb_pair, (active_bin_array_idx + 2) as i64).0,
        )
    };

    let (bin_array_bitmap_extension, _bump) = derive_bin_array_bitmap_extension(lb_pair);
    let bin_array_bitmap_extension = if program
        .rpc()
        .get_account(&bin_array_bitmap_extension)
        .is_err()
    {
        None
    } else {
        Some(bin_array_bitmap_extension)
    };

    let (event_authority, _bump) =
        Pubkey::find_program_address(&[b"__event_authority"], &lb_clmm::ID);

    let accounts = accounts::Swap {
        lb_pair,
        bin_array_bitmap_extension,
        reserve_x: lb_pair_state.reserve_x,
        reserve_y: lb_pair_state.reserve_y,
        token_x_mint: lb_pair_state.token_x_mint,
        token_y_mint: lb_pair_state.token_y_mint,
        token_x_program: anchor_spl::token::ID,
        token_y_program: anchor_spl::token::ID,
        user: payer.pubkey(),
        user_token_in,
        user_token_out,
        oracle: lb_pair_state.oracle,
        host_fee_in: Some(lb_clmm::ID),
        event_authority,
        program: lb_clmm::ID,
    };

    let ix = instruction::Swap {
        amount_in,
        min_amount_out: state.get_min_out_amount_with_slippage_rate(amount_in, swap_for_y)?,
    };

    let remaining_accounts = vec![
        AccountMeta {
            is_signer: false,
            is_writable: true,
            pubkey: bin_array_0,
        },
        AccountMeta {
            is_signer: false,
            is_writable: true,
            pubkey: bin_array_1,
        },
        AccountMeta {
            is_signer: false,
            is_writable: true,
            pubkey: bin_array_2,
        },
    ];

    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

    let builder = program.request();
    let builder = builder
        .instruction(compute_budget_ix)
        .accounts(accounts)
        .accounts(remaining_accounts)
        .args(ix);

    if is_simulation {
        let response = simulate_transaction(vec![&payer], payer.pubkey(), &program, &builder)?;
        println!("{:?}", response);
        return Ok(SwapEvent {
            lb_pair: Pubkey::default(),
            from: Pubkey::default(),
            start_bin_id: 0,
            end_bin_id: 0,
            amount_in: 0,
            amount_out: 0,
            swap_for_y,
            fee: 0,
            protocol_fee: 0,
            fee_bps: 0,
            host_fee: 0,
        });
    }

    let signature = send_tx(vec![&payer], payer.pubkey(), &program, &builder)?;
    info!("swap {amount_in} {swap_for_y} {signature}");

    // TODO should handle if cannot get swap eevent
    let swap_event = parse_swap_event(&program, signature)?;

    Ok(swap_event)
}