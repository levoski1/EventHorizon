#![no_std]
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype,
    token, Address, Env,
};

// Precision scalar for share math (avoids integer division rounding to zero)
const SCALAR: i128 = 1_000_000;

// ── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Asset,          // underlying token address
    TotalAssets,    // i128 – total assets under management (principal + accrued yield)
    TotalShares,    // i128 – total vault shares outstanding
    Shares(Address),// i128 – shares held by a depositor
    Paused,         // bool – emergency withdrawal guard
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contractevent]
pub struct Deposit {
    pub depositor: Address,
    pub assets: i128,
    pub shares: i128,
}

#[contractevent]
pub struct Withdraw {
    pub withdrawer: Address,
    pub assets: i128,
    pub shares: i128,
}

/// Emitted when the admin books yield gains into the vault (rebalance signal).
#[contractevent]
pub struct YieldAccrued {
    pub added_assets: i128,
    pub new_total_assets: i128,
}

/// Emitted to signal that funds should be moved to a higher-yield strategy.
#[contractevent]
pub struct RebalanceSignal {
    pub total_assets: i128,
    pub suggested_target: Address,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct YieldVault;

#[contractimpl]
impl YieldVault {
    // ── Setup ─────────────────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address, asset: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Asset, &asset);
        env.storage().instance().set(&DataKey::TotalAssets, &0i128);
        env.storage().instance().set(&DataKey::TotalShares, &0i128);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    // ── EIP-4626-style accounting ─────────────────────────────────────────────

    /// Deposit `assets` of the underlying token; receive vault shares in return.
    pub fn deposit(env: Env, depositor: Address, assets: i128) -> i128 {
        depositor.require_auth();
        Self::_require_not_paused(&env);
        if assets <= 0 { panic!("Assets must be positive"); }

        let shares = Self::_assets_to_shares(&env, assets);

        // Pull assets from depositor
        let asset: Address = env.storage().instance().get(&DataKey::Asset).unwrap();
        token::Client::new(&env, &asset)
            .transfer(&depositor, &env.current_contract_address(), &assets);

        // Update state
        Self::_add_shares(&env, &depositor, shares);
        Self::_set_total_assets(&env, Self::_total_assets(&env) + assets);
        Self::_set_total_shares(&env, Self::_total_shares(&env) + shares);

        env.events().publish_event(&Deposit { depositor, assets, shares });
        shares
    }

    /// Burn `shares` and receive the proportional underlying assets back.
    pub fn withdraw(env: Env, withdrawer: Address, shares: i128) -> i128 {
        withdrawer.require_auth();
        Self::_require_not_paused(&env);
        if shares <= 0 { panic!("Shares must be positive"); }

        let held = Self::_shares_of(&env, &withdrawer);
        if shares > held { panic!("Insufficient shares"); }

        let assets = Self::_shares_to_assets(&env, shares);
        if assets <= 0 { panic!("Zero asset redemption"); }

        // Update state before transfer (checks-effects-interactions)
        Self::_sub_shares(&env, &withdrawer, shares);
        Self::_set_total_assets(&env, Self::_total_assets(&env) - assets);
        Self::_set_total_shares(&env, Self::_total_shares(&env) - shares);

        let asset: Address = env.storage().instance().get(&DataKey::Asset).unwrap();
        token::Client::new(&env, &asset)
            .transfer(&env.current_contract_address(), &withdrawer, &assets);

        env.events().publish_event(&Withdraw { withdrawer, assets, shares });
        assets
    }

    // ── Yield & rebalance ─────────────────────────────────────────────────────

    /// Admin deposits external yield gains into the vault (must transfer tokens in).
    /// This increases the share price without minting new shares.
    pub fn accrue_yield(env: Env, added_assets: i128) {
        Self::_require_admin(&env);
        if added_assets <= 0 { panic!("Must be positive"); }

        // Pull the yield tokens from admin into the vault
        let asset: Address = env.storage().instance().get(&DataKey::Asset).unwrap();
        token::Client::new(&env, &asset)
            .transfer(&env.storage().instance().get::<_, Address>(&DataKey::Admin).unwrap(),
                      &env.current_contract_address(), &added_assets);

        let new_total = Self::_total_assets(&env) + added_assets;
        Self::_set_total_assets(&env, new_total);

        env.events().publish_event(&YieldAccrued { added_assets, new_total_assets: new_total });
    }

    /// Admin signals that funds should be rebalanced to a new strategy target.
    /// Does not move funds on-chain — the off-chain worker (EventHorizon) reacts
    /// to the `RebalanceSignal` event and executes the actual movement.
    pub fn signal_rebalance(env: Env, suggested_target: Address) {
        Self::_require_admin(&env);
        let total_assets = Self::_total_assets(&env);
        env.events().publish_event(&RebalanceSignal { total_assets, suggested_target });
    }

    // ── Emergency guard ───────────────────────────────────────────────────────

    /// Pause/unpause deposits and withdrawals. Admin only.
    pub fn set_paused(env: Env, paused: bool) {
        Self::_require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &paused);
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    pub fn total_assets(env: Env) -> i128 { Self::_total_assets(&env) }
    pub fn total_shares(env: Env) -> i128 { Self::_total_shares(&env) }
    pub fn shares_of(env: Env, addr: Address) -> i128 { Self::_shares_of(&env, &addr) }

    /// How many assets a given share count redeems for right now.
    pub fn preview_redeem(env: Env, shares: i128) -> i128 {
        Self::_shares_to_assets(&env, shares)
    }

    /// How many shares a given asset deposit would mint right now.
    pub fn preview_deposit(env: Env, assets: i128) -> i128 {
        Self::_assets_to_shares(&env, assets)
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    /// shares = assets * total_shares / total_assets  (1:1 when vault is empty)
    fn _assets_to_shares(env: &Env, assets: i128) -> i128 {
        let ts = Self::_total_shares(env);
        let ta = Self::_total_assets(env);
        if ts == 0 || ta == 0 {
            assets * SCALAR // initial deposit: 1 asset = SCALAR shares
        } else {
            assets.checked_mul(ts).expect("overflow")
                  .checked_div(ta).expect("div zero")
        }
    }

    /// assets = shares * total_assets / total_shares
    fn _shares_to_assets(env: &Env, shares: i128) -> i128 {
        let ts = Self::_total_shares(env);
        let ta = Self::_total_assets(env);
        if ts == 0 { return 0; }
        shares.checked_mul(ta).expect("overflow")
              .checked_div(ts).expect("div zero")
    }

    fn _total_assets(env: &Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalAssets).unwrap_or(0)
    }
    fn _total_shares(env: &Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalShares).unwrap_or(0)
    }
    fn _set_total_assets(env: &Env, v: i128) {
        env.storage().instance().set(&DataKey::TotalAssets, &v);
    }
    fn _set_total_shares(env: &Env, v: i128) {
        env.storage().instance().set(&DataKey::TotalShares, &v);
    }
    fn _shares_of(env: &Env, addr: &Address) -> i128 {
        env.storage().persistent().get(&DataKey::Shares(addr.clone())).unwrap_or(0)
    }
    fn _add_shares(env: &Env, addr: &Address, amount: i128) {
        let cur = Self::_shares_of(env, addr);
        env.storage().persistent().set(&DataKey::Shares(addr.clone()), &(cur + amount));
    }
    fn _sub_shares(env: &Env, addr: &Address, amount: i128) {
        let cur = Self::_shares_of(env, addr);
        env.storage().persistent().set(&DataKey::Shares(addr.clone()), &(cur - amount));
    }
    fn _require_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();
    }
    fn _require_not_paused(env: &Env) {
        let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        if paused { panic!("Vault is paused"); }
    }
}

mod test;
