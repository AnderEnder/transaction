use std::default::Default;

use rust_decimal::Decimal;

use crate::entry::{TransactionEntry, TransactionEntryType};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub tx_type: TransactionType,
    pub account_id: u16,
    pub tx_id: u32,
    pub amount: Decimal,
    pub status: TransactionStatus,
}

impl TryFrom<TransactionEntry> for Transaction {
    type Error = ConvertionError;

    fn try_from(value: TransactionEntry) -> Result<Self, Self::Error> {
        Ok(Transaction {
            tx_type: value.entry_type.try_into()?,
            account_id: value.account_id,
            tx_id: value.tx_id,
            amount: value.amount.ok_or(ConvertionError::MissingAmount)?,
            status: TransactionStatus::Completed,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
}

impl TryFrom<TransactionEntryType> for TransactionType {
    type Error = ConvertionError;

    fn try_from(value: TransactionEntryType) -> Result<Self, Self::Error> {
        match value {
            TransactionEntryType::Deposit => Ok(TransactionType::Deposit),
            TransactionEntryType::Withdrawal => Ok(TransactionType::Withdrawal),
            _ => Err(ConvertionError::InvalidTransactionType),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum TransactionStatus {
    #[default]
    Completed,
    Disputed,
    Resolved,
    Chargebacked,
}

#[derive(Error, Debug)]
pub enum ConvertionError {
    #[error("Invalid transaction type for conversion")]
    InvalidTransactionType,
    #[error("Missing amount for transaction")]
    MissingAmount,
}
