use {
    crate::helpers::{
        common::{FREEZE_AUTHORITY, MINT_DECIMALS},
        create_mint_builder::CreateMintBuilder,
    },
    helpers::{confidential_builder::ConfidentialMintBuilder, create_mint_builder::TokenProgram},
    mollusk_svm::result::Check,
    solana_account::Account,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    spl_pod::primitives::{PodBool, PodU64},
    spl_token_2022::{extension::PodStateWithExtensions, pod::PodMint, state::Mint},
    spl_token_wrap::{
        error::TokenWrapError, get_wrapped_mint_address, get_wrapped_mint_authority,
        state::Backpointer,
    },
};

pub mod helpers;

#[test]
fn test_successful_spl_token_to_confidential_transfer_mint() {
    let unwrapped_mint_address = Pubkey::new_unique();
    let wrapped_token_program_id = spl_token_2022::id();
    let wrapped_mint_address =
        get_wrapped_mint_address(&unwrapped_mint_address, &wrapped_token_program_id);

    

    let result = ConfidentialMintBuilder::default()
        .unwrapped_mint_addr(unwrapped_mint_address)
        .unwrapped_token_program(TokenProgram::SplToken)
        .wrapped_mint_addr(wrapped_mint_address)
        .wrapped_token_program(TokenProgram::SplToken2022)
        .execute_confidential_transfer_mint();

    // Assert state of resulting wrapped mint account

    assert_eq!(result.wrapped_mint.account.owner, spl_token_2022::id());

    let wrapped_mint_data =
        PodStateWithExtensions::<PodMint>::unpack(&result.wrapped_mint.account.data)
            .unwrap()
            .base;

    assert_eq!(wrapped_mint_data.decimals, MINT_DECIMALS);
    let expected_mint_authority = get_wrapped_mint_authority(&wrapped_mint_address);
    assert_eq!(
        wrapped_mint_data
            .mint_authority
            .ok_or(ProgramError::InvalidAccountData)
            .unwrap(),
        expected_mint_authority,
    );
    assert_eq!(wrapped_mint_data.supply, PodU64::from(0));
    assert_eq!(wrapped_mint_data.is_initialized, PodBool::from_bool(true));
    assert_eq!(
        wrapped_mint_data
            .freeze_authority
            .ok_or(ProgramError::InvalidAccountData)
            .unwrap(),
        FREEZE_AUTHORITY
    );

    // Assert state of resulting backpointer account

    assert_eq!(
        result.wrapped_backpointer.account.owner,
        spl_token_wrap::id()
    );

    let backpointer =
        bytemuck::from_bytes::<Backpointer>(&result.wrapped_backpointer.account.data[..]);
    assert_eq!(backpointer.unwrapped_mint, unwrapped_mint_address);
}
