#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let asset_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_id = env.register(YieldVault, ());
    YieldVaultClient::new(&env, &contract_id).initialize(&admin, &asset_addr);
    (env, admin, asset_addr, contract_id)
}

fn mint_to(env: &Env, _admin: &Address, asset: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, asset).mint(to, &amount);
}

#[test]
fn test_deposit_and_withdraw_roundtrip() {
    let (env, admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &admin, &asset, &user, 1000);

    let shares = client.deposit(&user, &1000i128);
    assert_eq!(shares, 1000 * SCALAR);
    assert_eq!(client.total_assets(), 1000);
    assert_eq!(client.shares_of(&user), 1000 * SCALAR);

    let returned = client.withdraw(&user, &shares);
    assert_eq!(returned, 1000);
    assert_eq!(client.total_assets(), 0);
    assert_eq!(client.shares_of(&user), 0);
    assert_eq!(TokenClient::new(&env, &asset).balance(&user), 1000);
}

#[test]
fn test_yield_accrual_increases_share_price() {
    let (env, admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    mint_to(&env, &admin, &asset, &u1, 1000);
    mint_to(&env, &admin, &asset, &u2, 1000);

    // u1 deposits 1000 → gets 1_000_000 shares (initial 1:SCALAR)
    let s1 = client.deposit(&u1, &1000i128);
    assert_eq!(s1, 1000 * SCALAR);

    // Admin accrues 1000 yield (mints to admin first, then admin transfers in)
    mint_to(&env, &admin, &asset, &admin, 1000);
    client.accrue_yield(&1000i128);
    assert_eq!(client.total_assets(), 2000);
    assert_eq!(client.total_shares(), 1000 * SCALAR);

    // u2 deposits 1000 → share price is now 2000/1_000_000 per share
    // new shares = 1000 * 1_000_000 / 2000 = 500_000
    let s2 = client.deposit(&u2, &1000i128);
    assert_eq!(s2, 500 * SCALAR);

    // total_assets=3000, total_shares=1_500_000
    // u1 redeems 1_000_000 shares → 1_000_000 * 3000 / 1_500_000 = 2000
    let u1_assets = client.withdraw(&u1, &s1);
    assert_eq!(u1_assets, 2000);

    // u2 redeems 500_000 shares → 500_000 * 1000 / 500_000 = 1000
    let u2_assets = client.withdraw(&u2, &s2);
    assert_eq!(u2_assets, 1000);
}

#[test]
fn test_rebalance_signal_emitted() {
    let (env, admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &admin, &asset, &user, 500);
    client.deposit(&user, &500i128);

    let target = Address::generate(&env);
    // Just verifying it doesn't panic and executes (event checked via snapshot)
    client.signal_rebalance(&target);
}

#[test]
#[should_panic(expected = "Vault is paused")]
fn test_paused_blocks_deposit() {
    let (env, _admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &_admin, &asset, &user, 100);
    client.set_paused(&true);
    client.deposit(&user, &100i128);
}

#[test]
#[should_panic(expected = "Vault is paused")]
fn test_paused_blocks_withdraw() {
    let (env, _admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &_admin, &asset, &user, 100);
    let shares = client.deposit(&user, &100i128);
    client.set_paused(&true);
    client.withdraw(&user, &shares);
}

#[test]
#[should_panic(expected = "Insufficient shares")]
fn test_withdraw_more_than_owned_panics() {
    let (env, admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &admin, &asset, &user, 100);
    let shares = client.deposit(&user, &100i128);
    client.withdraw(&user, &(shares + 1));
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_double_init_panics() {
    let (env, admin, asset, vault) = setup();
    YieldVaultClient::new(&env, &vault).initialize(&admin, &asset);
}

#[test]
fn test_preview_functions_match_actual() {
    let (env, admin, asset, vault) = setup();
    let client = YieldVaultClient::new(&env, &vault);
    let user = Address::generate(&env);
    mint_to(&env, &admin, &asset, &user, 1000);

    let preview_shares = client.preview_deposit(&1000i128);
    let actual_shares = client.deposit(&user, &1000i128);
    assert_eq!(preview_shares, actual_shares);

    let preview_assets = client.preview_redeem(&actual_shares);
    let actual_assets = client.withdraw(&user, &actual_shares);
    assert_eq!(preview_assets, actual_assets);
}
