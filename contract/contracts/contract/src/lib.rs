#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    symbol_short, Address, Env, String, Vec,
};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BountyStatus {
    Open,
    InReview,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Bounty {
    pub id: u64,
    pub creator: Address,
    pub title: String,
    pub description: String,
    pub reward: i128,        // in stroops (1 XLM = 10_000_000)
    pub token: Address,      // XLM or any SEP-41 token
    pub status: BountyStatus,
    pub hunter: Option<Address>,
    pub submission: Option<String>,
    pub created_at: u64,
    pub deadline: u64,
}

#[contracttype]
pub enum DataKey {
    BountyCount,
    Bounty(u64),
    CreatorBounties(Address),
    HunterBounties(Address),
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct BountyPlatform;

#[contractimpl]
impl BountyPlatform {
    /// Create a new bounty. Reward tokens are locked in escrow immediately.
    pub fn create_bounty(
        env: Env,
        creator: Address,
        title: String,
        description: String,
        reward: i128,
        token: Address,
        deadline: u64,
    ) -> u64 {
        creator.require_auth();

        assert!(reward > 0, "Reward must be positive");
        assert!(
            deadline > env.ledger().timestamp(),
            "Deadline must be in the future"
        );

        // Lock reward in contract (escrow)
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.transfer(&creator, &env.current_contract_address(), &reward);

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::BountyCount)
            .unwrap_or(0u64)
            + 1;

        let bounty = Bounty {
            id,
            creator: creator.clone(),
            title,
            description,
            reward,
            token,
            status: BountyStatus::Open,
            hunter: None,
            submission: None,
            created_at: env.ledger().timestamp(),
            deadline,
        };

        env.storage().instance().set(&DataKey::BountyCount, &id);
        env.storage().persistent().set(&DataKey::Bounty(id), &bounty);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorBounties(creator.clone()))
            .unwrap_or(Vec::new(&env));
        list.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::CreatorBounties(creator), &list);

        env.events()
            .publish((symbol_short!("CREATED"), id), (bounty.reward,));

        id
    }

    /// Hunter submits work. Bounty moves to InReview.
    pub fn submit_work(
        env: Env,
        hunter: Address,
        bounty_id: u64,
        submission: String,
    ) {
        hunter.require_auth();

        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        assert!(bounty.status == BountyStatus::Open, "Bounty is not open");
        assert!(
            env.ledger().timestamp() <= bounty.deadline,
            "Deadline has passed"
        );
        assert!(bounty.creator != hunter, "Creator cannot self-submit");

        bounty.hunter = Some(hunter.clone());
        bounty.submission = Some(submission);
        bounty.status = BountyStatus::InReview;

        env.storage()
            .persistent()
            .set(&DataKey::Bounty(bounty_id), &bounty);

        let mut list: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::HunterBounties(hunter.clone()))
            .unwrap_or(Vec::new(&env));
        list.push_back(bounty_id);
        env.storage()
            .persistent()
            .set(&DataKey::HunterBounties(hunter.clone()), &list);

        env.events()
            .publish((symbol_short!("SUBMITTED"), bounty_id), (hunter,));
    }

    /// Creator approves — escrowed reward is released to the hunter.
    pub fn approve_submission(env: Env, bounty_id: u64) {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();

        assert!(
            bounty.status == BountyStatus::InReview,
            "No submission to approve"
        );

        let hunter = bounty.hunter.clone().expect("No hunter on record");

        let token_client = soroban_sdk::token::Client::new(&env, &bounty.token);
        token_client.transfer(
            &env.current_contract_address(),
            &hunter,
            &bounty.reward,
        );

        bounty.status = BountyStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Bounty(bounty_id), &bounty);

        env.events()
            .publish((symbol_short!("APPROVED"), bounty_id), (hunter, bounty.reward));
    }

    /// Creator rejects — bounty returns to Open so new hunters can apply.
    pub fn reject_submission(env: Env, bounty_id: u64) {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();

        assert!(
            bounty.status == BountyStatus::InReview,
            "No submission to reject"
        );

        bounty.hunter = None;
        bounty.submission = None;
        bounty.status = BountyStatus::Open;

        env.storage()
            .persistent()
            .set(&DataKey::Bounty(bounty_id), &bounty);

        env.events()
            .publish((symbol_short!("REJECTED"), bounty_id), ());
    }

    /// Creator cancels an open bounty and reclaims the escrowed reward.
    pub fn cancel_bounty(env: Env, bounty_id: u64) {
        let mut bounty: Bounty = env
            .storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        bounty.creator.require_auth();

        assert!(
            bounty.status == BountyStatus::Open,
            "Can only cancel an open bounty"
        );

        let token_client = soroban_sdk::token::Client::new(&env, &bounty.token);
        token_client.transfer(
            &env.current_contract_address(),
            &bounty.creator,
            &bounty.reward,
        );

        bounty.status = BountyStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Bounty(bounty_id), &bounty);

        env.events()
            .publish((symbol_short!("CANCELLED"), bounty_id), ());
    }

    // ─── View functions ───────────────────────────────────────────────────

    pub fn get_bounty(env: Env, bounty_id: u64) -> Bounty {
        env.storage()
            .persistent()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found")
    }

    pub fn get_bounty_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::BountyCount)
            .unwrap_or(0)
    }

    pub fn get_creator_bounties(env: Env, creator: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::CreatorBounties(creator))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_hunter_bounties(env: Env, hunter: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::HunterBounties(hunter))
            .unwrap_or(Vec::new(&env))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env, String,
    };

    fn create_env() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, BountyPlatform);
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(token_admin.clone());

        let creator = Address::generate(&env);
        let hunter = Address::generate(&env);

        let token = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token.mint(&creator, &1_000_000_000i128);

        (env, contract_id, token_id, creator, hunter)
    }

    #[test]
    fn test_create_bounty() {
        let (env, contract_id, token_id, creator, _) = create_env();
        let client = BountyPlatformClient::new(&env, &contract_id);

        let id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Fix critical bug"),
            &String::from_str(&env, "Memory leak in module X"),
            &100_000_000i128,
            &token_id,
            &(env.ledger().timestamp() + 86_400),
        );

        assert_eq!(id, 1);
        let b = client.get_bounty(&1);
        assert_eq!(b.status, BountyStatus::Open);
        assert_eq!(b.reward, 100_000_000);
    }

    #[test]
    fn test_full_lifecycle() {
        let (env, contract_id, token_id, creator, hunter) = create_env();
        let client = BountyPlatformClient::new(&env, &contract_id);

        let id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Write docs"),
            &String::from_str(&env, "Full API documentation"),
            &50_000_000i128,
            &token_id,
            &(env.ledger().timestamp() + 86_400),
        );

        client.submit_work(
            &hunter,
            &id,
            &String::from_str(&env, "https://github.com/myorg/docs-pr"),
        );

        let b = client.get_bounty(&id);
        assert_eq!(b.status, BountyStatus::InReview);

        client.approve_submission(&id);

        let b = client.get_bounty(&id);
        assert_eq!(b.status, BountyStatus::Completed);
    }

    #[test]
    fn test_cancel_bounty() {
        let (env, contract_id, token_id, creator, _) = create_env();
        let client = BountyPlatformClient::new(&env, &contract_id);

        let id = client.create_bounty(
            &creator,
            &String::from_str(&env, "Design work"),
            &String::from_str(&env, "Logo design"),
            &20_000_000i128,
            &token_id,
            &(env.ledger().timestamp() + 86_400),
        );

        client.cancel_bounty(&id);
        let b = client.get_bounty(&id);
        assert_eq!(b.status, BountyStatus::Cancelled);
    }
}