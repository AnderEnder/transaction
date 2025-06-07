use thiserror::Error;

use crate::transaction::ConvertionError;

#[derive(Error, Debug)]
pub enum PaymentError {
    #[error("Insufficient funds for transaction")]
    InsufficientFunds,
    #[error("Insufficient hold funds for transaction")]
    InsufficientHoldFunds,
    #[error("Account is locked: {0}")]
    AccountLocked(u16),
    #[error("Account not found: {0}")]
    AccountNotFound(u16),
    #[error("Transaction not found")]
    TransactionNotFound,
    #[error("Invalid transaction type for operation")]
    InvalidTransactionType,
    #[error("Transaction already exists")]
    TransactionAlreadyExists,
    #[error("Transaction already disputed")]
    TransactionAlreadyDisputed,
    #[error("Transaction is not disputed")]
    TransactionIsNotDisputed,
    #[error("Invalid entry for transaction conversion")]
    InvalidEntryForConversion(ConvertionError),
}

impl From<ConvertionError> for PaymentError {
    fn from(error: ConvertionError) -> Self {
        Self::InvalidEntryForConversion(error)
    }
}
