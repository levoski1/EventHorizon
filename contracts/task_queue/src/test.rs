#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Bytes, Env};

fn setup() -> (Env, Address, TaskQueueContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let contract_id = env.register(TaskQueueContract, ());
    let client = TaskQueueContractClient::new(&env, &contract_id);
    (env, owner, client)
}

fn payload(env: &Env) -> Bytes {
    Bytes::from_slice(env, b"action:webhook:https://example.com")
}

// ── register ─────────────────────────────────────────────────────────────────

#[test]
fn test_register_increments_id() {
    let (env, owner, client) = setup();
    let p = payload(&env);

    let id0 = client.register(&owner, &p, &1000);
    let id1 = client.register(&owner, &p, &2000);

    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
    assert_eq!(client.next_id(), 2);
}

#[test]
fn test_register_stores_task() {
    let (env, owner, client) = setup();
    let p = payload(&env);

    let id = client.register(&owner, &p, &500);
    let task = client.get_task(&id);

    assert_eq!(task.id, id);
    assert_eq!(task.owner, owner);
    assert_eq!(task.trigger_at, 500);
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(task.payload, p);
}

// ── bump ─────────────────────────────────────────────────────────────────────

#[test]
fn test_bump_extends_trigger_time() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);

    client.bump(&id, &2000);

    assert_eq!(client.get_task(&id).trigger_at, 2000);
}

#[test]
#[should_panic(expected = "new trigger must be later")]
fn test_bump_rejects_earlier_time() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);
    client.bump(&id, &999);
}

#[test]
#[should_panic(expected = "task not pending")]
fn test_bump_cancelled_task_panics() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);
    client.cancel(&id);
    client.bump(&id, &2000);
}

// ── cancel ────────────────────────────────────────────────────────────────────

#[test]
fn test_cancel_sets_status() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);

    client.cancel(&id);

    assert_eq!(client.get_task(&id).status, TaskStatus::Cancelled);
}

#[test]
#[should_panic(expected = "task not pending")]
fn test_double_cancel_panics() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);
    client.cancel(&id);
    client.cancel(&id);
}

// ── trigger ───────────────────────────────────────────────────────────────────

#[test]
fn test_trigger_after_due_time() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);

    env.ledger().set_timestamp(1000);
    client.trigger(&id);

    assert_eq!(client.get_task(&id).status, TaskStatus::Triggered);
}

#[test]
#[should_panic(expected = "not yet due")]
fn test_trigger_before_due_time_panics() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);

    env.ledger().set_timestamp(999);
    client.trigger(&id);
}

#[test]
#[should_panic(expected = "task not pending")]
fn test_trigger_cancelled_task_panics() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);
    client.cancel(&id);

    env.ledger().set_timestamp(1000);
    client.trigger(&id);
}

#[test]
#[should_panic(expected = "task not pending")]
fn test_double_trigger_panics() {
    let (env, owner, client) = setup();
    let id = client.register(&owner, &payload(&env), &1000);

    env.ledger().set_timestamp(1000);
    client.trigger(&id);
    client.trigger(&id);
}

// ── full lifecycle ────────────────────────────────────────────────────────────

#[test]
fn test_full_lifecycle_register_bump_trigger() {
    let (env, owner, client) = setup();
    let p = payload(&env);

    // Register at t=0, due at t=500
    let id = client.register(&owner, &p, &500);
    assert_eq!(client.get_task(&id).status, TaskStatus::Pending);

    // Bump to t=1000
    client.bump(&id, &1000);
    assert_eq!(client.get_task(&id).trigger_at, 1000);

    // Too early
    env.ledger().set_timestamp(999);
    // (would panic if triggered here — covered by separate test)

    // Exactly on time
    env.ledger().set_timestamp(1000);
    client.trigger(&id);
    assert_eq!(client.get_task(&id).status, TaskStatus::Triggered);
}
