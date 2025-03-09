use {
    super::{
        common::{setup_confidential_transfer_account, setup_confidential_transfer_mint},
        create_mint_builder::{KeyedAccount, TokenProgram},
    },
    crate::helpers::common::{init_mollusk, setup_mint},
    mollusk_svm::{result::Check, Mollusk},
    solana_account::Account,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_sdk::{signature::Keypair, signer::Signer},
    solana_sdk_ids::system_program,
    solana_zk_sdk::encryption::elgamal::ElGamalKeypair,
    spl_token_wrap::{
        get_wrapped_mint_address, get_wrapped_mint_authority, get_wrapped_mint_backpointer_address,
        instruction::{create_confidential_mint, create_mint, wrap},
    },
    std::convert::TryInto,
};

pub struct CreateMintResult {
    pub unwrapped_mint: KeyedAccount,
    pub wrapped_mint: KeyedAccount,
    pub wrapped_backpointer: KeyedAccount,
}

pub struct ConfidentialMintBuilder<'a> {
    mollusk: Mollusk,
    wrapped_token_program: TokenProgram,
    wrapped_token_program_addr: Option<Pubkey>,
    unwrapped_mint_addr: Option<Pubkey>,
    unwrapped_mint_account: Option<Account>,
    unwrapped_token_program: TokenProgram,
    wrapped_mint_addr: Option<Pubkey>,
    wrapped_mint_account: Option<Account>,
    backpointer_addr: Option<Pubkey>,
    backpointer_account: Option<Account>,
    auditor_keypair: Option<ElGamalKeypair>,
    idempotent: bool,
    checks: Vec<Check<'a>>,
}

impl Default for ConfidentialMintBuilder<'_> {
    fn default() -> Self {
        Self {
            mollusk: init_mollusk(),
            wrapped_token_program: TokenProgram::SplToken2022,
            wrapped_token_program_addr: None,
            unwrapped_mint_addr: None,
            unwrapped_mint_account: None,
            unwrapped_token_program: TokenProgram::SplToken,
            wrapped_mint_addr: None,
            wrapped_mint_account: None,
            backpointer_addr: None,
            backpointer_account: None,
            auditor_keypair: None,
            idempotent: false,
            checks: vec![],
        }
    }
}

impl<'a> ConfidentialMintBuilder<'a> {
    pub fn wrapped_token_program(mut self, program: TokenProgram) -> Self {
        self.wrapped_token_program = program;
        self
    }

    pub fn unwrapped_token_program(mut self, program: TokenProgram) -> Self {
        self.unwrapped_token_program = program;
        self
    }

    pub fn token_program_addr(mut self, key: Pubkey) -> Self {
        self.wrapped_token_program_addr = Some(key);
        self
    }

    pub fn unwrapped_mint_addr(mut self, key: Pubkey) -> Self {
        self.unwrapped_mint_addr = Some(key);
        self
    }

    pub fn wrapped_mint_addr(mut self, key: Pubkey) -> Self {
        self.wrapped_mint_addr = Some(key);
        self
    }

    pub fn wrapped_mint_account(mut self, account: Account) -> Self {
        self.wrapped_mint_account = Some(account);
        self
    }

    pub fn backpointer_addr(mut self, key: Pubkey) -> Self {
        self.backpointer_addr = Some(key);
        self
    }
    pub fn backpointer_account(mut self, account: Account) -> Self {
        self.backpointer_account = Some(account);
        self
    }

    pub fn auditor_keypair(mut self, keypair: ElGamalKeypair) -> Self {
        self.auditor_keypair = Some(keypair);
        self
    }

    pub fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }

    pub fn check(mut self, check: Check<'a>) -> Self {
        self.checks.push(check);
        self
    }

    pub fn execute_confidential_transfer_mint(mut self) -> CreateMintResult {
        let unwrapped_mint_addr = self.unwrapped_mint_addr.unwrap_or_else(Pubkey::new_unique);
        let wrapped_token_program_id = self
            .wrapped_token_program_addr
            .unwrap_or_else(|| self.wrapped_token_program.id());

        let unwrapped_mint_account = self.unwrapped_mint_account.clone().unwrap_or_else(|| {
            setup_mint(
                self.unwrapped_token_program,
                &self.mollusk.sysvars.rent,
                Pubkey::new_unique(),
            )
        });

        let wrapped_mint_addr = self.wrapped_mint_addr.unwrap_or_else(|| {
            get_wrapped_mint_address(&unwrapped_mint_addr, &wrapped_token_program_id)
        });

        // setup the confidential transfer mint
        let wrapped_mint_account = self.wrapped_mint_account.unwrap_or(Account {
            lamports: 100_000_000,
            ..Default::default()
        });

        let wrapped_backpointer_address = self
            .backpointer_addr
            .unwrap_or_else(|| get_wrapped_mint_backpointer_address(&wrapped_mint_addr));

        let wrapped_backpointer_account = self.backpointer_account.unwrap_or(Account {
            lamports: 100_000_000,
            ..Default::default()
        });
        let auditor_keypair = if let Some(auditor_keypair) = self.auditor_keypair {
            let pk: [u8; 32] = From::from(auditor_keypair.pubkey_owned());
            pk
        } else {
            [0u8; 32]
        };

        let confidential_token_account_owner = Keypair::new();

        let confidential_token_account = setup_confidential_transfer_account(
            &confidential_token_account_owner,
            &KeyedAccount {
                key: wrapped_mint_addr,
                account: wrapped_mint_account.clone(),
            },
        );

        let unwrapped_token_account = {
            let mut account = Account {
                lamports: 100_000_000,
                owner: spl_token::id(),
                data: vec![0; spl_token::state::Account::LEN],
                ..Default::default()
            };
            let token_account = spl_token::state::Account {
                mint: unwrapped_mint_addr,
                owner: confidential_token_account_owner.pubkey(),
                amount: 100_000_000,
                delegate: None.into(),
                state: spl_token::state::AccountState::Initialized,
                is_native: None.into(),
                delegated_amount: 0,
                close_authority: None.into(),
            };
            spl_token::state::Account::pack(token_account, &mut account.data).unwrap();
            KeyedAccount {
                account: account,
                key: spl_associated_token_account::get_associated_token_address(
                    &confidential_token_account_owner.pubkey(),
                    &unwrapped_mint_addr,
                ),
            }
        };
        let escrow_token_account = {
            let mut escrow_account = Account {
                lamports: 100_000_00,
                owner: spl_token::id(),
                data: vec![0; spl_token::state::Account::LEN],
                ..Default::default()
            };
            let escrow_token = spl_token::state::Account {
                mint: unwrapped_mint_addr,
                owner: get_wrapped_mint_authority(&wrapped_mint_addr),
                amount: 0,
                delegate: None.into(),
                state: spl_token::state::AccountState::Initialized,
                is_native: None.into(),
                delegated_amount: 0,
                close_authority: None.into()
            };
            spl_token::state::Account::pack(escrow_token, &mut escrow_account.data).unwrap();
            KeyedAccount {
                key: Pubkey::new_unique(),
                account: escrow_account,
            }
        };
        let instruction = create_confidential_mint(
            &spl_token_wrap::id(),
            &wrapped_mint_addr,
            &wrapped_backpointer_address,
            &unwrapped_mint_addr,
            &wrapped_token_program_id,
            self.idempotent,
            true,
            [0u8; 32],
            auditor_keypair,
        );

        let mut keyed_token_program = match self.wrapped_token_program {
            TokenProgram::SplToken => mollusk_svm_programs_token::token::keyed_account(),
            TokenProgram::SplToken2022 => mollusk_svm_programs_token::token2022::keyed_account(),
        };
        keyed_token_program.0 = wrapped_token_program_id;

        let wrap_ix = wrap(
            &spl_token_wrap::id(),
            &confidential_token_account.key,
            &wrapped_mint_addr,
            &get_wrapped_mint_authority(&wrapped_mint_addr),
            &spl_token::id(),
            &spl_token_2022::id(),
            &unwrapped_token_account.key,
            &unwrapped_mint_addr,
            &escrow_token_account.key,
            &confidential_token_account_owner.pubkey(),
            &[],
            1_000
        );
        let accounts = &[
            (wrapped_mint_addr, wrapped_mint_account),
            (wrapped_backpointer_address, wrapped_backpointer_account),
            (unwrapped_mint_addr, unwrapped_mint_account),
            (
                system_program::id(),
                Account {
                    executable: true,
                    ..Default::default()
                },
            ),
            keyed_token_program,
            (
                confidential_token_account.key,
                confidential_token_account.account,
            ),
            (unwrapped_token_account.key, unwrapped_token_account.account),
            (escrow_token_account.key, escrow_token_account.account),
            mollusk_svm_programs_token::token::keyed_account(),
            (
                confidential_token_account_owner.pubkey(),
                Account {
                    executable: false,
                    ..Default::default()
                }
            ),
            (
                get_wrapped_mint_authority(&wrapped_mint_addr),
                Account {
                    executable: false,
                    ..Default::default()
                }
            )
        ];
        for account in accounts {
            println!("{}", account.0);
        }
        if self.checks.is_empty() {
            self.checks.push(Check::success());
        }

        let result = self
            .mollusk
            .process_and_validate_instruction_chain(
                &[
                    (&instruction, &self.checks),
                    (&wrap_ix, &self.checks),
                ], accounts);

        CreateMintResult {
            unwrapped_mint: KeyedAccount {
                key: unwrapped_mint_addr,
                account: result.get_account(&unwrapped_mint_addr).unwrap().clone(),
            },
            wrapped_mint: KeyedAccount {
                key: wrapped_mint_addr,
                account: result.get_account(&wrapped_mint_addr).unwrap().clone(),
            },
            wrapped_backpointer: KeyedAccount {
                key: wrapped_backpointer_address,
                account: result
                    .get_account(&wrapped_backpointer_address)
                    .unwrap()
                    .clone(),
            },
        }
    }
}
