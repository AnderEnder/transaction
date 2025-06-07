use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TransactionEntry {
    #[serde(rename = "type")]
    pub entry_type: TransactionEntryType,
    #[serde(rename = "client")]
    pub account_id: u16,
    #[serde(rename = "tx")]
    pub tx_id: u32,
    #[serde(deserialize_with = "csv::invalid_option")]
    pub amount: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionEntryType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}
