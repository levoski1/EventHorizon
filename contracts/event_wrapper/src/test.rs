#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

struct Setup {
    env: Env,
    admin: Address,
    underlying: Address,
    contract_id: Address,
    client: EventWrapperClient<'static>,
    asset: TokenClient<'static>,
    asset_admin: StellarAssetClient<'static>,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let underlying = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let asset_admin = StellarAssetClient::new(&env, &underlying);
    let asset = TokenClient::new(&env, &underlying);

    let contract_id = env.register(EventWrapper, ());
    let client = EventWrapperClient::new(&env, &contract_id);
    client.initialize(&admin, &underlying);

    Setup { env, admin, underlying, contract_id, client, asset, asset_admin }
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn test_metadata_mirrored() {
    let s = setup();
    // Stellar asset contracts expose "native" / "" by default in test env;
    // we just verify the wrapper returns the same values as the underlying.
    assert_eq!(s.client.name(), s.asset.name());
    assert_eq!(s.client.symbol(), s.asset.symbol());
    assert_eq!(s.client.decimals(), s.asset.decimals());
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_init_panics() {
    let s = setup();
    s.client.initialize(&s.admin, &s.underlying);
}

// ── wrap / unwrap ─────────────────────────────────────────────────────────────

#[test]
fn test_wrap_mints_wrapped_balance() {
    let s = setup();
    let user = Address::generate(&s.env);
    s.asset_admin.mint(&user, &1000);

    s.client.wrap(&user, &1000);

    assert_eq!(s.client.balance(&user), 1000);
    // Underlying tokens are now held by the contract.
    assert_eq!(s.asset.balance(&s.contract_id), 1000);
    assert_eq!(s.asset.balance(&user), 0);
}

#[test]
fn test_unwrap_returns_underlying() {
    let s = setup();
    let user = Address::generate(&s.env);
    s.asset_admin.mint(&user, &500);

    s.client.wrap(&user, &500);
    s.client.unwrap(&user, &300);

    assert_eq!(s.client.balance(&user), 200);
    assert_eq!(s.asset.balance(&user), 300);
    assert_eq!(s.asset.balance(&s.contract_id), 200);
}

#[test]
#[should_panic(expected = "insufficient wrapped balance")]
fn test_unwrap_more_than_balance_panics() {
    let s = setup();
    let user = Address::generate(&s.env);
    s.asset_admin.mint(&user, &100);
    s.client.wrap(&user, &100);
    s.client.unwrap(&user, &101);
}

// ── SEP-41: transfer ──────────────────────────────────────────────────────────

#[test]
fn test_transfer() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    s.asset_admin.mint(&alice, &200);
    s.client.wrap(&alice, &200);

    s.client.transfer(&alice, &bob, &150);

    assert_eq!(s.client.balance(&alice), 50);
    assert_eq!(s.client.balance(&bob), 150);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_transfer_insufficient_balance_panics() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    s.asset_admin.mint(&alice, &100);
    s.client.wrap(&alice, &100);
    s.client.transfer(&alice, &bob, &101);
}

// ── SEP-41: approve / transfer_from ──────────────────────────────────────────

#[test]
fn test_approve_and_transfer_from() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let spender = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    s.asset_admin.mint(&alice, &500);
    s.client.wrap(&alice, &500);

    s.client.approve(&alice, &spender, &300, &9999);
    assert_eq!(s.client.allowance(&alice, &spender), 300);

    s.client.transfer_from(&spender, &alice, &bob, &200);

    assert_eq!(s.client.allowance(&alice, &spender), 100);
    assert_eq!(s.client.balance(&alice), 300);
    assert_eq!(s.client.balance(&bob), 200);
}

#[test]
#[should_panic(expected = "insufficient allowance")]
fn test_transfer_from_exceeds_allowance_panics() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let spender = Address::generate(&s.env);
    let bob = Address::generate(&s.env);
    s.asset_admin.mint(&alice, &500);
    s.client.wrap(&alice, &500);
    s.client.approve(&alice, &spender, &100, &9999);
    s.client.transfer_from(&spender, &alice, &bob, &101);
}

// ── SEP-41: burn ──────────────────────────────────────────────────────────────

#[test]
fn test_burn() {
    let s = setup();
    let user = Address::generate(&s.env);
    s.asset_admin.mint(&user, &400);
    s.client.wrap(&user, &400);

    s.client.burn(&user, &100);

    assert_eq!(s.client.balance(&user), 300);
}

#[test]
fn test_burn_from() {
    let s = setup();
    let alice = Address::generate(&s.env);
    let spender = Address::generate(&s.env);
    s.asset_admin.mint(&alice, &400);
    s.client.wrap(&alice, &400);
    s.client.approve(&alice, &spender, &200, &9999);

    s.client.burn_from(&spender, &alice, &150);

    assert_eq!(s.client.balance(&alice), 250);
    assert_eq!(s.client.allowance(&alice, &spender), 50);
}

// ── SEP-41: admin ─────────────────────────────────────────────────────────────

#[test]
fn test_set_admin() {
    let s = setup();
    let new_admin = Address::generate(&s.env);

    s.client.set_admin(&new_admin);

    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn test_mint_by_admin() {
    let s = setup();
    let user = Address::generate(&s.env);

    s.client.mint(&user, &777);

    assert_eq!(s.client.balance(&user), 777);
}
