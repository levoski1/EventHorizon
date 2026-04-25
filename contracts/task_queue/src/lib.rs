#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, Symbol};

// ── Storage keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Monotonic counter for task IDs (instance).
    NextId,
    /// Per-task record (persistent).
    Task(u64),
}

// ── Data types ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Triggered,
    Cancelled,
}

/// Full task record stored on-chain and echoed in every event.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Task {
    pub id: u64,
    pub owner: Address,
    /// Arbitrary payload the off-chain worker needs (ABI-encoded call, URL, etc.)
    pub payload: Bytes,
    /// Ledger timestamp after which the task is considered expired/ready.
    pub trigger_at: u64,
    pub status: TaskStatus,
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct TaskQueueContract;

#[contractimpl]
impl TaskQueueContract {
    // ── Write functions ───────────────────────────────────────────────────

    /// Register a new task. Returns the assigned task ID.
    ///
    /// * `trigger_at` – ledger timestamp at which the task becomes actionable.
    /// * `payload`    – opaque bytes forwarded verbatim to the off-chain worker.
    pub fn register(env: Env, owner: Address, payload: Bytes, trigger_at: u64) -> u64 {
        owner.require_auth();

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);
        let next = id.checked_add(1).expect("id overflow");
        env.storage().instance().set(&DataKey::NextId, &next);

        let task = Task {
            id,
            owner: owner.clone(),
            payload: payload.clone(),
            trigger_at,
            status: TaskStatus::Pending,
        };
        env.storage().persistent().set(&DataKey::Task(id), &task);

        env.events().publish(
            (symbol_short!("registered"), owner),
            task,
        );

        id
    }

    /// Extend (bump) the trigger time of a pending task.
    /// Only the task owner may call this.
    pub fn bump(env: Env, task_id: u64, new_trigger_at: u64) {
        let mut task: Task = env
            .storage()
            .persistent()
            .get(&DataKey::Task(task_id))
            .expect("task not found");

        task.owner.require_auth();

        if task.status != TaskStatus::Pending {
            panic!("task not pending");
        }
        if new_trigger_at <= task.trigger_at {
            panic!("new trigger must be later");
        }

        task.trigger_at = new_trigger_at;
        env.storage().persistent().set(&DataKey::Task(task_id), &task);

        env.events().publish(
            (symbol_short!("bumped"), task.owner.clone()),
            task,
        );
    }

    /// Cancel a pending task. Only the task owner may call this.
    pub fn cancel(env: Env, task_id: u64) {
        let mut task: Task = env
            .storage()
            .persistent()
            .get(&DataKey::Task(task_id))
            .expect("task not found");

        task.owner.require_auth();

        if task.status != TaskStatus::Pending {
            panic!("task not pending");
        }

        task.status = TaskStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Task(task_id), &task);

        env.events().publish(
            (symbol_short!("cancelled"), task.owner.clone()),
            task,
        );
    }

    /// Mark a pending task as triggered. Called by the off-chain worker (or
    /// any caller) once `env.ledger().timestamp() >= task.trigger_at`.
    ///
    /// Emits a `triggered` event with the full payload so the worker can act.
    pub fn trigger(env: Env, task_id: u64) {
        let mut task: Task = env
            .storage()
            .persistent()
            .get(&DataKey::Task(task_id))
            .expect("task not found");

        if task.status != TaskStatus::Pending {
            panic!("task not pending");
        }
        if env.ledger().timestamp() < task.trigger_at {
            panic!("not yet due");
        }

        task.status = TaskStatus::Triggered;
        env.storage().persistent().set(&DataKey::Task(task_id), &task);

        env.events().publish(
            (Symbol::new(&env, "triggered"), task.owner.clone()),
            task,
        );
    }

    // ── Read functions ────────────────────────────────────────────────────

    pub fn get_task(env: Env, task_id: u64) -> Task {
        env.storage()
            .persistent()
            .get(&DataKey::Task(task_id))
            .expect("task not found")
    }

    pub fn next_id(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::NextId).unwrap_or(0)
    }
}

mod test;
