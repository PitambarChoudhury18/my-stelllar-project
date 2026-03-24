#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, Vec, symbol_short
};

#[contract]
pub struct BountyContract;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Bounty(u32),
    Count,
}

#[contracttype]
#[derive(Clone)]
pub struct Bounty {
    pub creator: Address,
    pub description: String,
    pub reward: i128,
    pub completed: bool,
    pub hunter: Option<Address>,
}

#[contractimpl]
impl BountyContract {

    // Create bounty
    pub fn create_bounty(
        env: Env,
        creator: Address,
        description: String,
        reward: i128,
    ) -> u32 {
        creator.require_auth();

        let mut count: u32 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);

        let bounty = Bounty {
            creator: creator.clone(),
            description,
            reward,
            completed: false,
            hunter: None,
        };

        env.storage().instance().set(&DataKey::Bounty(count), &bounty);
        env.storage().instance().set(&DataKey::Count, &(count + 1));

        // 🔔 Emit event
        env.events().publish(
            (symbol_short!("created"), creator),
            count,
        );

        count
    }

    // Complete bounty
    pub fn complete_bounty(env: Env, bounty_id: u32, hunter: Address) {
        hunter.require_auth();

        let mut bounty: Bounty = env
            .storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found");

        if bounty.completed {
            panic!("Already completed");
        }

        bounty.completed = true;
        bounty.hunter = Some(hunter.clone());

        env.storage().instance().set(&DataKey::Bounty(bounty_id), &bounty);

        // 🔔 Emit event
        env.events().publish(
            (symbol_short!("completed"), hunter),
            bounty_id,
        );
    }

    // Get bounty
    pub fn get_bounty(env: Env, bounty_id: u32) -> Bounty {
        env.storage()
            .instance()
            .get(&DataKey::Bounty(bounty_id))
            .expect("Bounty not found")
    }

    // Total count
    pub fn get_count(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Count).unwrap_or(0)
    }
}