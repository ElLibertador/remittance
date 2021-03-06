#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, SubMsg, WasmMsg,
};

use cw2::set_contract_version;
use cw20::{Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::msg::{
    CreateMsg, DetailsResponse, ExecuteMsg, InstantiateMsg, ListResponse, QueryMsg, ReceiveMsg, FeedbackMsg, ArbitrateMsg
};
use crate::state::{all_escrow_ids, Escrow, GenericBalance, ESCROWS, TrustMetrics};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-escrow";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // no setup
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ElArbitrate(id, msg) => el_arbitrate(deps, env, info, msg, id),
        ExecuteMsg::CCreate(msg) => c_create(deps, msg, Balance::from(info.funds), &info.sender),
        ExecuteMsg::FAccept { id } => f_accept(deps, env, info, id),
        ExecuteMsg::CCancel { id } => c_cancel(deps, env, info, id),
        ExecuteMsg::FUnaccept { id } => f_unaccept(deps, env, info, id),
        ExecuteMsg::CChange(msg) => c_change(deps, env, info, msg),
        ExecuteMsg::FComplete { id } => f_complete(deps, env, info, id),
        ExecuteMsg::CReqArbitration { id } => c_request_arbitration(deps, env, info, id),
        ExecuteMsg::CComplete { id } => c_complete(deps, env, info, id),
        ExecuteMsg::CFeedback(id, msg) => c_feedback(deps, env, info, msg, id),
        ExecuteMsg::FFeedback(id, msg) => f_feedback(deps, env, info, msg, id),
    }
}

pub fn el_arbitrate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ArbitrateMsg,
    id: String,
) -> Result<Response, ContractError> {
    // ArbitrateMsg contains the wallet of whom to send the funds to
    return Err(ContractError::Unauthorized {});
}

pub fn c_create(
    deps: DepsMut,
    msg: CreateMsg,
    balance: Balance,
    sender: &Addr,
) -> Result<Response, ContractError> {
    if balance.is_empty() {
        return Err(ContractError::EmptyBalance {});
    }

    let mut cw20_whitelist = msg.addr_whitelist(deps.api)?;

    let escrow_balance = match balance {
        Balance::Native(balance) => GenericBalance {
            native: balance.0,
            cw20: vec![],
        },
        Balance::Cw20(token) => {
            // make sure the token sent is on the whitelist by default
            if !cw20_whitelist.iter().any(|t| t == &token.address) {
                cw20_whitelist.push(token.address.clone())
            }
            GenericBalance {
                native: vec![],
                cw20: vec![token],
            }
        }
    };

    // TODO: Make sure this can be at max 7 days from now, since we don't want to keep contracts more than 7 days old
    let end_time = msg.end_time;
    
    let escrow = Escrow {
        arbiter: deps.api.addr_validate(&msg.arbiter)?,
        fulfiller: sender.clone(),
        creator: sender.clone(),
        end_height: msg.end_height,
        end_time: end_time,
        balance: escrow_balance,
        exchange_rate: msg.exchange_rate,
        cw20_whitelist,
        required_trust_metrics: msg.required_trust_metrics,
        is_listed: true,
        is_canceled: false,
        is_accepted: false,
        is_fulfilled: false,
        is_in_arbitration: false,
        is_completed: false,
        time_created: Some(0),
        time_accepted: Some(0),
        time_fulfilled: Some(0),
        time_arbitration_started: Some(0),
    };

    // try to store it, fail if the id was already in use
    ESCROWS.update(deps.storage, &msg.id, |existing| match existing {
        None => Ok(escrow),
        Some(_) => Err(ContractError::AlreadyInUse {}),
    })?;

    let res = Response::new().add_attributes(vec![("action", "create"), ("id", msg.id.as_str())]);
    Ok(res)
}

pub fn f_accept(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let mut escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender == escrow.creator {
        // The contract creator can't accept their own contract
        return Err(ContractError::Unauthorized {});
    } 
    // We check if the contract is in a state where it can be accepted
    else if !escrow.is_listed {
        return Err(ContractError::NotListed {});
    }
    // We have to check if trust metrics of the sender wallet are tolerable
    else if escrow.required_trust_metrics.is_higher(get_trust_metrics(&info.sender)) {
        return Err(ContractError::TrustMetricsInsufficient {});
    } 
    else {
        // We set the message sender as the contract fulfiller
        escrow.fulfiller = info.sender;
        // TODO: Set escrow.is_accepted to true
        let res = Response::new().add_attributes(vec![("action", "accept"), ("id", id.as_str())]);
        return Ok(res)
    }
}

pub fn c_cancel(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let mut escrow = ESCROWS.load(deps.storage, &id)?;
    if !escrow.is_accept_expired(&env) && info.sender != escrow.creator {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_accepted {
        return Err(ContractError::CantUnaccept {});
    } else {
        escrow.is_listed = false;
        escrow.is_canceled = true;
        // we delete the escrow
        ESCROWS.remove(deps.storage, &id);

        Ok(Response::new()
            .add_attribute("action", "unaccept")
            .add_attribute("id", id))
    }
}

pub fn f_unaccept(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let mut escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.fulfiller {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_accepted {
        return Err(ContractError::CantUnaccept {});
    } else {
        // Remove the fulfiller
        escrow.fulfiller = info.sender;

        Ok(Response::new()
            .add_attribute("action", "unaccept")
            .add_attribute("id", id))
    }
}

pub fn c_change(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CreateMsg,
) -> Result<Response, ContractError> {
    // TODO: Implement contract changes
    return Err(ContractError::Unauthorized {})
}

pub fn f_complete(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.fulfiller {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_accepted {
        return Err(ContractError::CantFulfill {});
    } else {
        // TODO: Change state like below
        // escrow.is_fulfilled = true;
        Ok(Response::new()
            .add_attribute("action", "fulfill")
            .add_attribute("id", id))
    }
}

pub fn c_request_arbitration(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.creator {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_fulfilled {
        return Err(ContractError::NotFulfilled {});
    } else {
        // TODO: Change state like below
        // escrow.is_in_arbitration = true;
        Ok(Response::new()
            .add_attribute("action", "request_arbitration")
            .add_attribute("id", id))
    }
}

pub fn c_complete(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, ContractError> {
    let escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.creator {
        Err(ContractError::Unauthorized {})
    } 
    else if !escrow.is_fulfilled | escrow.is_completed {
        Err(ContractError::Expired {})
    } else {
        // we delete the escrow
        ESCROWS.remove(deps.storage, &id);

        // send all tokens out
        let messages: Vec<SubMsg> = send_tokens(&escrow.fulfiller, &escrow.balance)?;

        Ok(Response::new()
            .add_attribute("action", "creator_complete")
            .add_attribute("id", id)
            .add_attribute("to", escrow.fulfiller)
            .add_submessages(messages))
    }
}

pub fn c_feedback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: FeedbackMsg,
    id: String,
) -> Result<Response, ContractError> {
    // TODO: Implement feedback state for contract
    let escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.creator {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_completed {
        return Err(ContractError::NotComplete {});
    } else {
        Ok(Response::new()
            .add_attribute("action", "creator_feedback")
            .add_attribute("id", id)
        )
    }
}

pub fn f_feedback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: FeedbackMsg,
    id: String,
) -> Result<Response, ContractError> {
    // TODO: Implement feedback state for contract
    let escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.fulfiller {
        return Err(ContractError::Unauthorized {});
    } else if !escrow.is_completed {
        return Err(ContractError::NotComplete {});
    } else {
        Ok(Response::new()
            .add_attribute("action", "fulfiller_feedback")
            .add_attribute("id", id)
        )
    }
}

fn get_trust_metrics(sender: &Addr) -> TrustMetrics {
    return TrustMetrics {
        percent_completed: 95,
        percent_satisfied: 90,
        avg_volume: 100,
        avg_completion_speed: 600000,
        total_volume: 2000,
        total_completed: 20,
    }
}

fn send_tokens(to: &Addr, balance: &GenericBalance) -> StdResult<Vec<SubMsg>> {
    let native_balance = &balance.native;
    let mut msgs: Vec<SubMsg> = if native_balance.is_empty() {
        vec![]
    } else {
        vec![SubMsg::new(BankMsg::Send {
            to_address: to.into(),
            amount: native_balance.to_vec(),
        })]
    };

    let cw20_balance = &balance.cw20;
    let cw20_msgs: StdResult<Vec<_>> = cw20_balance
        .iter()
        .map(|c| {
            let msg = Cw20ExecuteMsg::Transfer {
                recipient: to.into(),
                amount: c.amount,
            };
            let exec = SubMsg::new(WasmMsg::Execute {
                contract_addr: c.address.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            });
            Ok(exec)
        })
        .collect();
    msgs.append(&mut cw20_msgs?);
    Ok(msgs)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::List {} => to_binary(&query_list(deps)?),
        QueryMsg::Details { id } => to_binary(&query_details(deps, id)?),
    }
}

fn query_details(deps: Deps, id: String) -> StdResult<DetailsResponse> {
    let escrow = ESCROWS.load(deps.storage, &id)?;

    let cw20_whitelist = escrow.human_whitelist();

    // transform tokens
    let native_balance = escrow.balance.native;

    let cw20_balance: StdResult<Vec<_>> = escrow
        .balance
        .cw20
        .into_iter()
        .map(|token| {
            Ok(Cw20Coin {
                address: token.address.into(),
                amount: token.amount,
            })
        })
        .collect();

    let details = DetailsResponse {
        id,
        arbiter: escrow.arbiter.into(),
        fulfiller: escrow.fulfiller.into(),
        creator: escrow.creator.into(),
        end_height: escrow.end_height,
        end_time: escrow.end_time,
        native_balance,
        cw20_balance: cw20_balance?,
        cw20_whitelist,
    };
    Ok(details)
}

fn query_list(deps: Deps) -> StdResult<ListResponse> {
    Ok(ListResponse {
        escrows: all_escrow_ids(deps.storage)?,
    })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coin, coins, CosmosMsg, StdError, Uint128};

    use crate::msg::ExecuteMsg::TopUp;

    use super::*;

    #[test]
    fn happy_path_native() {
        let mut deps = mock_dependencies(&[]);

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: String::from("recd"),
            end_time: None,
            end_height: Some(123456),
            cw20_whitelist: None,
        };
        let sender = String::from("source");
        let balance = coins(100, "tokens");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::Create(create.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the details is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: String::from("recd"),
                source: String::from("source"),
                end_height: Some(123456),
                end_time: None,
                native_balance: balance.clone(),
                cw20_balance: vec![],
                cw20_whitelist: vec![],
            }
        );

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(("action", "approve"), res.attributes[0]);
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: create.recipient,
                amount: balance,
            }))
        );

        // second attempt fails (not found)
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::NotFound { .. })));
    }

    #[test]
    fn happy_path_cw20() {
        let mut deps = mock_dependencies(&[]);

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: String::from("recd"),
            end_time: None,
            end_height: None,
            cw20_whitelist: Some(vec![String::from("other-token")]),
        };
        let receive = Cw20ReceiveMsg {
            sender: String::from("source"),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::Create(create.clone())).unwrap(),
        };
        let token_contract = String::from("my-cw20-token");
        let info = mock_info(&token_contract, &[]);
        let msg = ExecuteMsg::Receive(receive.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the whitelist is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: String::from("recd"),
                source: String::from("source"),
                end_height: None,
                end_time: None,
                native_balance: vec![],
                cw20_balance: vec![Cw20Coin {
                    address: String::from("my-cw20-token"),
                    amount: Uint128::new(100),
                }],
                cw20_whitelist: vec![String::from("other-token"), String::from("my-cw20-token")],
            }
        );

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(("action", "approve"), res.attributes[0]);
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient,
            amount: receive.amount,
        };
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_contract,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );

        // second attempt fails (not found)
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::NotFound { .. })));
    }

    #[test]
    fn add_tokens_proper() {
        let mut tokens = GenericBalance::default();
        tokens.add_tokens(Balance::from(vec![coin(123, "atom"), coin(789, "eth")]));
        tokens.add_tokens(Balance::from(vec![coin(456, "atom"), coin(12, "btc")]));
        assert_eq!(
            tokens.native,
            vec![coin(579, "atom"), coin(789, "eth"), coin(12, "btc")]
        );
    }

    #[test]
    fn add_cw_tokens_proper() {
        let mut tokens = GenericBalance::default();
        let bar_token = Addr::unchecked("bar_token");
        let foo_token = Addr::unchecked("foo_token");
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: foo_token.clone(),
            amount: Uint128::new(12345),
        }));
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: bar_token.clone(),
            amount: Uint128::new(777),
        }));
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: foo_token.clone(),
            amount: Uint128::new(23400),
        }));
        assert_eq!(
            tokens.cw20,
            vec![
                Cw20CoinVerified {
                    address: foo_token,
                    amount: Uint128::new(35745),
                },
                Cw20CoinVerified {
                    address: bar_token,
                    amount: Uint128::new(777),
                }
            ]
        );
    }

    #[test]
    fn top_up_mixed_tokens() {
        let mut deps = mock_dependencies(&[]);

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // only accept these tokens
        let whitelist = vec![String::from("bar_token"), String::from("foo_token")];

        // create an escrow with 2 native tokens
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: String::from("recd"),
            end_time: None,
            end_height: None,
            cw20_whitelist: Some(whitelist),
        };
        let sender = String::from("source");
        let balance = vec![coin(100, "fee"), coin(200, "stake")];
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::Create(create.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // top it up with 2 more native tokens
        let extra_native = vec![coin(250, "random"), coin(300, "stake")];
        let info = mock_info(&sender, &extra_native);
        let top_up = ExecuteMsg::TopUp {
            id: create.id.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // top up with one foreign token
        let bar_token = String::from("bar_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(7890),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&bar_token, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // top with a foreign token not on the whitelist
        // top up with one foreign token
        let baz_token = String::from("baz_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(7890),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&baz_token, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, top_up).unwrap_err();
        assert_eq!(err, ContractError::NotInWhitelist {});

        // top up with second foreign token
        let foo_token = String::from("foo_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(888),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&foo_token, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(("action", "approve"), res.attributes[0]);
        assert_eq!(3, res.messages.len());

        // first message releases all native coins
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: create.recipient.clone(),
                amount: vec![coin(100, "fee"), coin(500, "stake"), coin(250, "random")],
            }))
        );

        // second one release bar cw20 token
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient.clone(),
            amount: Uint128::new(7890),
        };
        assert_eq!(
            res.messages[1],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: bar_token,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );

        // third one release foo cw20 token
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient,
            amount: Uint128::new(888),
        };
        assert_eq!(
            res.messages[2],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: foo_token,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );
    }

    #[test]
    fn creator_calls_the_creator_complete_function() {
        // We create a mutable variable named deps and set it equal to the state returned by the function named mock_dependencies
        let mut deps = mock_dependencies(&[]);

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        // Our contract is instantiated by ElLib
        let info = mock_info(&String::from("ElLib"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());
        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: String::from("fulfiller"),
            end_time: None,
            end_height: Some(123456),
            cw20_whitelist: None,
        };
        // We set the sender to "creator"
        let sender = String::from("creator");
        // We give the sender a balance of 100 tokens
        let balance = coins(100, "tokens");
        let info = mock_info(&sender, &balance);
        // We called the Execute Message: Create and give it a copy of our CreateMsg
        let msg = ExecuteMsg::Create(create.clone());
        // We call the execute function with our ExecuteMsg::Create and unwrap it's result
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        // We make sure no error messages are returned
        assert_eq!(0, res.messages.len());
        // We check that the tuple with "action" and "create" are returned, signifying execute_create returned Ok
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the details is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: String::from("fulfiller"),
                // Check that "creator" is the source
                source: String::from("creator"),
                end_height: Some(123456),
                end_time: None,
                native_balance: balance.clone(),
                cw20_balance: vec![],
                cw20_whitelist: vec![],
            }
        );

        /* Here we have the fulfiller try to call the creator complete method, which would be fraud */
        // We get the contract id
        let id = create.id.clone();
        // We make a message coming from the fulfiller
        let info = mock_info(&create.recipient, &[]);
        // Get the results of calling execute with the fulfiller as the message signer
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::CreatorComplete { id }).unwrap_err();
        // We check that the response is 
        assert_eq!(err, ContractError::Unauthorized {});

        /* Here is where we call the CreatorComplete Execution Method */
        // We get the id of the contract we've created
        let id = create.id.clone();
        // We make our message info come from the creator
        let info = mock_info(&sender, &[]);
        // We send an ExecuteMsg of type CreatorComplete
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::CreatorComplete { id }).unwrap();
        // We check that the response has a single message
        assert_eq!(1, res.messages.len());
        // We check the response attributes match the ones from creator_complete
        assert_eq!(("action", "creator_complete"), res.attributes[0]);
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: create.recipient,
                amount: balance,
            }))
        );

        // second attempt fails (not found)
        let id = create.id.clone();
        let info = mock_info(&sender, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::CreatorComplete { id }).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::NotFound { .. })));
    }
}
