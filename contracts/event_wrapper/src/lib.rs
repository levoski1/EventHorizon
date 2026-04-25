#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Symbol,
};

// ── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Address of the underlying Stellar asset (instance).
    Underlying,
    /// Admin address (instance).
    Admin,
    /// Cached name from underlying (instance).
    Name,
    /// Cached symbol from underlying (instance).
    Symbol,
    /// Cached decimals from underlying (instance).
    Decimals,
    /// Wrapped token balance per holder (persistent).
    Balance(Address),
    /// Approved allowance: (owner, spender) → amount (persistent).
    Allowance(Address, Address),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct EventWrapper;

#[contractimpl]
impl EventWrapper {
    // ── Initialisation ────────────────────────────────────────────────────

    /// Deploy the wrapper around an existing Stellar asset.
    /// Mirrors name / symbol / decimals from the underlying asset.
    pub fn initialize(env: Env, admin: Address, underlying: Address) {
        if env.storage().instance().has(&DataKey::Underlying) {
            panic!("already initialized");
        }
        admin.require_auth();

        // Mirror metadata from the underlying asset once.
        let asset = token::Client::new(&env, &underlying);
        let name = asset.name();
        let symbol = asset.symbol();
        let decimals = asset.decimals();

        env.storage().instance().set(&DataKey::Underlying, &underlying);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
    }

    // ── Wrap / Unwrap (non-SEP-41 extensions) ────────────────────────────

    /// Deposit `amount` of the underlying asset and mint wrapped tokens 1:1.
    /// Emits `Wrapped`.
    pub fn wrap(env: Env, account: Address, amount: i128) {
        account.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let underlying: Address = env.storage().instance().get(&DataKey::Underlying).expect("not initialized");
        // Pull underlying tokens from the caller into this contract.
        token::Client::new(&env, &underlying)
            .transfer(&account, &env.current_contract_address(), &amount);

        // Mint wrapped balance.
        let bal = Self::balance_of(&env, &account);
        env.storage().persistent().set(&DataKey::Balance(account.clone()), &(bal + amount));

        env.events().publish((symbol_short!("wrapped"), account), amount);
    }

    /// Burn `amount` of wrapped tokens and return the underlying asset 1:1.
    /// Emits `Unwrapped`.
    pub fn unwrap(env: Env, account: Address, amount: i128) {
        account.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let bal = Self::balance_of(&env, &account);
        if bal < amount {
            panic!("insufficient wrapped balance");
        }

        // Burn wrapped balance first (checks-effects-interactions).
        env.storage().persistent().set(&DataKey::Balance(account.clone()), &(bal - amount));

        let underlying: Address = env.storage().instance().get(&DataKey::Underlying).expect("not initialized");
        token::Client::new(&env, &underlying)
            .transfer(&env.current_contract_address(), &account, &amount);

        env.events().publish((symbol_short!("unwrapped"), account), amount);
    }

    // ── SEP-41: metadata ──────────────────────────────────────────────────

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&DataKey::Name).expect("not initialized")
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&DataKey::Symbol).expect("not initialized")
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Decimals).expect("not initialized")
    }

    // ── SEP-41: balances ──────────────────────────────────────────────────

    pub fn balance(env: Env, id: Address) -> i128 {
        Self::balance_of(&env, &id)
    }

    // ── SEP-41: allowances ────────────────────────────────────────────────

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Allowance(from, spender))
            .unwrap_or(0)
    }

    /// Approve `spender` to transfer up to `amount` from `from`.
    /// Emits the standard `approve` event **and** the extra `ApprovalRequested` event.
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        from.require_auth();
        if amount < 0 {
            panic!("amount must be non-negative");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Allowance(from.clone(), spender.clone()), &amount);

        // Standard SEP-41 approve event.
        env.events().publish(
            (Symbol::new(&env, "approve"), from.clone(), spender.clone()),
            (amount, expiration_ledger),
        );

        // Extra high-utility event for EventHorizon listeners.
        env.events().publish(
            (Symbol::new(&env, "ApprovalRequested"), from, spender),
            (amount, expiration_ledger),
        );
    }

    // ── SEP-41: transfers ─────────────────────────────────────────────────

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::do_transfer(&env, &from, &to, amount);

        env.events().publish(
            (Symbol::new(&env, "transfer"), from, to),
            amount,
        );
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();

        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        let current: i128 = env.storage().persistent().get(&allowance_key).unwrap_or(0);
        if current < amount {
            panic!("insufficient allowance");
        }
        env.storage().persistent().set(&allowance_key, &(current - amount));

        Self::do_transfer(&env, &from, &to, amount);

        env.events().publish(
            (Symbol::new(&env, "transfer"), from, to),
            amount,
        );
    }

    // ── SEP-41: mint / burn ───────────────────────────────────────────────

    /// Admin-only mint (e.g. for bridging scenarios).
    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("not initialized");
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let bal = Self::balance_of(&env, &to);
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(bal + amount));

        env.events().publish((Symbol::new(&env, "mint"), admin, to), amount);
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        Self::do_burn(&env, &from, amount);
        env.events().publish((symbol_short!("burn"), from), amount);
    }

    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();

        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        let current: i128 = env.storage().persistent().get(&allowance_key).unwrap_or(0);
        if current < amount {
            panic!("insufficient allowance");
        }
        env.storage().persistent().set(&allowance_key, &(current - amount));

        Self::do_burn(&env, &from, amount);
        env.events().publish((symbol_short!("burn"), from), amount);
    }

    // ── SEP-41: admin ─────────────────────────────────────────────────────

    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events().publish((symbol_short!("set_admin"), admin), new_admin);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).expect("not initialized")
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    fn balance_of(env: &Env, addr: &Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(addr.clone()))
            .unwrap_or(0)
    }

    fn do_transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let from_bal = Self::balance_of(env, from);
        if from_bal < amount {
            panic!("insufficient balance");
        }
        let to_bal = Self::balance_of(env, to);
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_bal - amount));
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(to_bal + amount));
    }

    fn do_burn(env: &Env, from: &Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let bal = Self::balance_of(env, from);
        if bal < amount {
            panic!("insufficient balance");
        }
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(bal - amount));
    }
}

mod test;
