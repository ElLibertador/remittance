use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Env, Order, StdError, StdResult, Storage, Timestamp};
use cw_storage_plus::Map;

use cw20::{Balance, Cw20CoinVerified};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct GenericBalance {
    pub native: Vec<Coin>,
    pub cw20: Vec<Cw20CoinVerified>,
}

impl GenericBalance {
    pub fn add_tokens(&mut self, add: Balance) {
        match add {
            Balance::Native(balance) => {
                for token in balance.0 {
                    let index = self.native.iter().enumerate().find_map(|(i, exist)| {
                        if exist.denom == token.denom {
                            Some(i)
                        } else {
                            None
                        }
                    });
                    match index {
                        Some(idx) => self.native[idx].amount += token.amount,
                        None => self.native.push(token),
                    }
                }
            }
            Balance::Cw20(token) => {
                let index = self.cw20.iter().enumerate().find_map(|(i, exist)| {
                    if exist.address == token.address {
                        Some(i)
                    } else {
                        None
                    }
                });
                match index {
                    Some(idx) => self.cw20[idx].amount += token.amount,
                    None => self.cw20.push(token),
                }
            }
        };
    }
}

pub struct TrustMetrics {
    pub percent_completed: u8, // Contracts
    pub percent_satisfied: u8, // Creator Feedback
    pub avg_volume: u32, // UST
    pub avg_completion_speed: u32, // Milliseconds
    pub total_volume: u32, // UST
    pub total_completed: u32, // Contracts
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Escrow {
    /// arbiter can decide to approve or refund the escrow
    pub arbiter: Addr,
    /// if is_completed, funds go to the fulfiller
    pub fulfiller: Addr,
    /// if canceled or arbitrated in favor of them, funds go to the creator
    pub creator: Addr,
    /// When end height set and block height exceeds this value, the escrow is expired.
    /// Once an escrow is expired, it can be returned to the original funder (via "refund").
    pub end_height: Option<u64>,
    /// When end time (in seconds since epoch 00:00:00 UTC on 1 January 1970) is set and
    /// block time exceeds this value, the escrow is expired.
    /// Once an escrow is expired, it can be returned to the original funder (via "refund").
    pub end_time: Option<u64>,
    /// Balance in Native and Cw20 tokens
    pub balance: GenericBalance,
    /// Exchange rate desired in Bolivares per UST
    pub exchange_rate: u128,
    /// All possible contracts that we accept tokens from
    pub cw20_whitelist: Vec<Addr>,
    /// Required Trust Metrics
    pub required_trust_metrics: TrustMetrics,
    /// States
    pub is_listed: bool,
    pub is_canceled: bool,
    pub is_accepted: bool,
    pub is_fulfilled: bool,
    pub is_in_arbitration: bool,
    pub is_completed: bool,
    /// State Timers
    pub time_created: Option<u64>,
    pub time_accepted: Option<u64>,
    pub time_fulfilled: Option<u64>,
    pub time_arbitration_started: Option<u64>,
}

impl TrustMetrics {
    pub fn is_higher(&self, fulfiller_trust_metrics: TrustMetrics) {
        let other = fulfiller_trust_metrics
        if self.percent_completed > other.percent_completed {
            false;
        }
        if self.percent_satisfied > other.percent_satisfied {
            false;
        }
        if self.avg_volume > other.avg_volume {
            false;
        }
        if self.avg_completion_speed < other.avg_completion_speed {
            false;
        }
        if self.total_volume > other.total_volume {
            false;
        }
        if self.total_completed > other.total_completed {
            false;
        }
        true;
    }
}

impl Escrow {
    pub fn is_expired(&self, env: &Env) -> bool {
        if let Some(end_height) = self.end_height {
            if env.block.height > end_height {
                return true;
            }
        }
        // We check if the current state's end_time is equal to the current block's end_time
        if let Some(end_time) = self.end_time {
            // If the current time of the current block is greater than the time converted from end_time
            if env.block.time > Timestamp::from_seconds(end_time) {
                // We set is_expired equal to true
                return true;
            }
        }

        false
    }

    pub fn is_accept_expired(&self, env: &Env) {
        // Check if the time since the fulfiller accepted has exceeded an hour
        return true;
    }

    pub fn is_fulfill_expired(&self, env: &Env) {
        // Check if the time since the fulfiller completed has exceeded an hour
        return true;
    }

    pub fn is_arbitration_expired(&self, env: &Env) {
        // Check if the time since the arbitration started has exceeded two days
        return true;
    }

    pub fn human_whitelist(&self) -> Vec<String> {
        self.cw20_whitelist.iter().map(|a| a.to_string()).collect()
    }
}

pub const ESCROWS: Map<&str, Escrow> = Map::new("escrow");

/// This returns the list of ids for all registered escrows
pub fn all_escrow_ids(storage: &dyn Storage) -> StdResult<Vec<String>> {
    ESCROWS
        .keys(storage, None, None, Order::Ascending)
        .map(|k| String::from_utf8(k).map_err(|_| StdError::invalid_utf8("parsing escrow key")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::MockStorage;

    #[test]
    fn no_escrow_ids() {
        let storage = MockStorage::new();
        let ids = all_escrow_ids(&storage).unwrap();
        assert_eq!(0, ids.len());
    }

    fn dummy_escrow() -> Escrow {
        Escrow {
            arbiter: Addr::unchecked("arb"),
            recipient: Addr::unchecked("recip"),
            source: Addr::unchecked("source"),
            end_height: None,
            end_time: None,
            balance: Default::default(),
            cw20_whitelist: vec![],
        }
    }

    #[test]
    fn all_escrow_ids_in_order() {
        let mut storage = MockStorage::new();
        ESCROWS
            .save(&mut storage, &"lazy", &dummy_escrow())
            .unwrap();
        ESCROWS
            .save(&mut storage, &"assign", &dummy_escrow())
            .unwrap();
        ESCROWS.save(&mut storage, &"zen", &dummy_escrow()).unwrap();

        let ids = all_escrow_ids(&storage).unwrap();
        assert_eq!(3, ids.len());
        assert_eq!(
            vec!["assign".to_string(), "lazy".to_string(), "zen".to_string()],
            ids
        )
    }
}
