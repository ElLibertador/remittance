# Remittance
A CosmWasm Smart Contract for people wanting to send Remittances to relatives in Venezuela. Staking UST in escrow to be sent to someone who fulfills it by sending Bolivars to the bank account indicated.

# El Libertador DeFi Remittance Contract

This contract acts as an automated, decentralized broker owned by its creator.

Each contract has a creator, fulfiller, and arbiter (El Libertador Staff).

It also has a unique ID (for referencing it later), and a timeout.

The basic function is the `creator: Addr` can `instantiate()` the contract, and send `amount: UST` which is put in escrow. They also determine an exchange `rate: u64` in Bolivars per UST they're willing to accept, and the `trust_requirements: TrustMetrics` they require from a contract fulfiller.

Any `fulfiller: Addr` can then `accept()` the contract for a small `fee: UST` (equal to the gas cost to handle the functions they'll be calling, in the case of either failure or fulfillment), and will have a period of `time: u32` to fulfill it before their tmeporarily fulfillment abilities are revoked and the contract is `available: bool` again for other people to accept and attempt to fulfill.

The fulfiller can call the `fulfilled()` method once they have fulfilled the contract in the amount of the `total_fiat: u64 = rate * amount`. The arbitration process and subsequent derogatory marks from calling this fraudulemntly should discourage that behavior.

The fulfiller can also `release()` the contract if they can't fulfill it for any reason.

Once the `fulfilled()` method is called, the creator can either `complete()` the contract which will `private release_funds(fulfiller)`, or `contest(reason: str)` the contract, in which case the `arbiter: Addr` (El Libertador Staff) will now have permission to `arbitrate(<creator | fulfiller>)` and it will `release_funds(<creator | fulfiller>)` to the person chosen by the arbiter.

After the contract is `closed: bool`, each party can leave a `review(satisfied: bool)` which other people will be able to see the results of on future contracts. Querying these reviews are the backbone of our trust metrics.

## Trust Metrics

Our `trust_requirements: TrustMetrics` are a parameter passed by the `creator: Addr` during `instantiate()`.

When a `fulfiller: Addr` tries to `accept()` the contract, we `private check_metrics(fulfiller)` of the `fulfiller` to see if theirs meet the minimum requirements.

```
TrustMetrics: {
 percent_completed: u8,
 total_completed: u32,
 percent_satisfied: u8,
 average_completion_time: u32, //in seconds
}
```

*!Under Construction!*: The `private check_metrics(fulfiller)` method is one we're still trying to understand how to best handle the performance of, given that it seems it'll have to search O(n) contracts ever instantiated by our template for each fulfiller that attempts to `accept()` a contract.

## Arbitration

If either the `creator | fulfiller` call the `contest(reason: str)` method, the `arbiter: Addr` will have to determine where the error lies, and either `release_funds(recipient: Addr)` to the creator or fulfiller, depending on if he determines the creator has been paid by the fulfiller or not.

This solution does not *scale*, but as we are in the early stages, we're still working on deciding the best methods for automating arbitration. We're considering a computer vision Oracle given Matt's extensive background in that field. Nerio's 5 years as a broker himself should make the manual task easy for him, as he already has methods to handle disputes. We're found, in practice, the percent of cases with conflict is low when there are trust based systems which incentivize users with trusted account to continue making an honest profit rather than lose their pristine trust metrics. Nerio, for example, has 100% trust ratings on the outdated, centralized platform localbitcoins.


## Running this contract

You will need Rust 1.44.1+ with `wasm32-unknown-unknown` target installed.

You can run unit tests on this via: 

`cargo test`

Once you are happy with the content, you can compile it to wasm via:

```
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/cw20_escrow.wasm .
ls -l cw20_escrow.wasm
sha256sum cw20_escrow.wasm
```