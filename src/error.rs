use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Contract is not listed as available for acceptance")]
    NotListed {},

    #[error("Sender Trust Metrics Not High Enough To Accept This Contract")]
    TrustMetricsInsufficient {},

    #[error("Escrow has already been accepted")]
    AlreadyAccepted {},

    #[error("Escrow can not be unaccepted now")]
    CantUnaccept {},

    #[error("The escrow either hasn't been accepted, or has already been fulfilled")]
    CantFulfill {},

    #[error("The escrow either hasn't been fulfilled yet, or has already been arbitrated")]
    NotFulfilled {},

    #[error("The escrow was never completed so a review can't be left")]
    NotComplete {},

    #[error("Only accepts tokens in the cw20_whitelist")]
    NotInWhitelist {},

    #[error("Escrow is expired")]
    Expired {},

    #[error("Send some coins to create an escrow")]
    EmptyBalance {},

    #[error("Escrow id already in use")]
    AlreadyInUse {},
}
