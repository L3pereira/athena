mod cancel_order;
mod get_depth;
mod get_exchange_info;
mod liquidity;
mod process_deposit;
mod process_withdrawal;
mod request_withdrawal;
mod submit_order;
mod swap;

pub use cancel_order::{CancelError, CancelOrderCommand, CancelOrderResult, CancelOrderUseCase};
pub use get_depth::{DepthError, DepthResult, GetDepthQuery, GetDepthUseCase};
pub use get_exchange_info::{ExchangeInfo, ExchangeInfoError, GetExchangeInfoUseCase};
pub use liquidity::{
    AddLiquidityCommand, AddLiquidityExecutionResult, LiquidityAddedEvent, LiquidityRemovedEvent,
    LiquidityUseCase, LiquidityUseCaseError, RemoveLiquidityCommand,
    RemoveLiquidityExecutionResult,
};
pub use process_deposit::{
    Deposit, DepositCreditedEvent, DepositId, DepositStatus, ProcessDepositError,
    ProcessDepositUseCase, ProcessDepositsResult, RegisterDepositAddressCommand,
};
pub use process_withdrawal::{
    AddConfirmationCommand, ConfirmWithdrawalCommand, FailWithdrawalCommand,
    ProcessWithdrawalCommand, ProcessWithdrawalError, ProcessWithdrawalResult,
    ProcessWithdrawalUseCase,
};
pub use request_withdrawal::{
    RequestWithdrawalCommand, RequestWithdrawalResult, RequestWithdrawalUseCase,
    WithdrawalUseCaseError,
};
pub use submit_order::{OrderError, SubmitOrderCommand, SubmitOrderResult, SubmitOrderUseCase};
pub use swap::{
    SwapCommand, SwapExecutedEvent, SwapExecutionResult, SwapQuote, SwapUseCase, SwapUseCaseError,
};
