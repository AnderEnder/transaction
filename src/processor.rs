use crate::entry::{TransactionEntry, TransactionEntryType};
use crate::error::PaymentError;
use crate::payments_engine::PaymentEngine;

use std::io::Read;
use std::iter::Iterator;

use csv::{ReaderBuilder, Trim};

#[inline]
pub fn process_csv_stream(engine: &mut PaymentEngine, reader: impl Read) {
    let mut binding = ReaderBuilder::new()
        .has_headers(true)
        .quoting(false)
        .trim(Trim::All)
        .flexible(true)
        .from_reader(reader);

    let stream = binding
        .deserialize()
        .inspect(|result: &Result<TransactionEntry, csv::Error>| {
            if let Err(e) = result {
                eprintln!("Error parsing transaction: {}", e);
            }
        })
        .filter_map(Result::ok);

    process_stream(engine, stream);
}

#[inline]
pub fn process_stream(engine: &mut PaymentEngine, stream: impl Iterator<Item = TransactionEntry>) {
    for transaction in stream {
        let result = process_entry(engine, transaction);

        result.unwrap_or_else(|e| {
            eprintln!("Error processing transaction: {}", e);
        });
    }
}

#[inline]
fn process_entry(
    engine: &mut PaymentEngine,
    transaction: TransactionEntry,
) -> Result<(), PaymentError> {
    let result: Result<(), PaymentError> = match transaction.entry_type {
        TransactionEntryType::Withdrawal | TransactionEntryType::Deposit => {
            engine.process_transaction(transaction.try_into()?)
        }
        TransactionEntryType::Dispute => {
            engine.process_dispute(transaction.account_id, transaction.tx_id)
        }
        TransactionEntryType::Resolve => {
            engine.process_resolve(transaction.account_id, transaction.tx_id)
        }
        TransactionEntryType::Chargeback => {
            engine.process_chargeback(transaction.account_id, transaction.tx_id)
        }
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::TransactionStatus;
    use rust_decimal::dec;

    #[test]
    fn test_process_csv_stream() {
        let mut engine = PaymentEngine::new();
        let data = "type, client, tx, amount\n\
                    deposit, 1, 1, 100.0\n\
                    withdrawal, 1, 2, 50.0\n\
                    dispute, 1, 1\n\
                    resolve, 1, 1\n\
                    chargeback, 1, 2";
        let reader = data.as_bytes();

        process_csv_stream(&mut engine, reader);

        assert_eq!(engine.accounts.len(), 1);
        assert_eq!(engine.transactions.len(), 1);
    }

    #[test]
    fn test_process_stream() {
        let mut engine = PaymentEngine::new();
        let transactions = vec![
            TransactionEntry {
                entry_type: TransactionEntryType::Deposit,
                account_id: 1,
                tx_id: 1,
                amount: Some(dec!(100.0)),
            },
            TransactionEntry {
                entry_type: TransactionEntryType::Withdrawal,
                account_id: 1,
                tx_id: 2,
                amount: Some(dec!(50.0)),
            },
            TransactionEntry {
                entry_type: TransactionEntryType::Dispute,
                account_id: 1,
                tx_id: 1,
                amount: None,
            },
        ];

        process_stream(&mut engine, transactions.into_iter());

        assert_eq!(engine.accounts.len(), 1);
        assert_eq!(engine.transactions.len(), 1);
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(50.0));
        assert_eq!(engine.transactions.get(&1).unwrap().len(), 2);
    }

    #[test]
    fn test_process_entry_duplicate() {
        let mut engine = PaymentEngine::new();
        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: Some(dec!(100.0)),
        };

        let result = process_entry(&mut engine, entry.clone());
        assert!(result.is_ok());

        let result = process_entry(&mut engine, entry);
        assert!(result.is_err(), "Should not allow duplicate transactions");

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: Some(dec!(1.0)),
        };

        let result = process_entry(&mut engine, entry.clone());
        assert!(result.is_ok());

        let result = process_entry(&mut engine, entry);
        assert!(result.is_err(), "Should not allow duplicate transactions");
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(99.0));

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: 1,
            tx_id: 3,
            amount: Some(dec!(50.0)),
        };
        process_entry(&mut engine, entry).unwrap();
        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Dispute,
            account_id: 1,
            tx_id: 3,
            amount: None,
        };
        let result = process_entry(&mut engine, entry.clone());
        assert!(result.is_ok(), "Dispute should be processed successfully");
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(50.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(149.0));
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(99.0));
        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&3).unwrap().status,
            TransactionStatus::Disputed
        );

        let result = process_entry(&mut engine, entry);
        assert!(result.is_err(), "Should not allow duplicate disputes");

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Resolve,
            account_id: 1,
            tx_id: 3,
            amount: None,
        };
        let result = process_entry(&mut engine, entry.clone());

        assert!(result.is_ok(), "Resolve should be processed successfully");
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(149.0));
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(149.0));
        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&3).unwrap().status,
            TransactionStatus::Resolved
        );

        let result = process_entry(&mut engine, entry);
        assert!(result.is_err(), "Should not allow duplicate resolves");

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Chargeback,
            account_id: 1,
            tx_id: 3,
            amount: None,
        };
        let result = process_entry(&mut engine, entry.clone());

        assert!(
            result.is_err(),
            "Chargeback should not be allowed after resolve"
        );
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(149.0));
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(149.0));
        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&3).unwrap().status,
            TransactionStatus::Resolved
        );
    }

    #[test]
    fn test_process_entry_duplicate_cachback() {
        let mut engine = PaymentEngine::new();
        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: Some(dec!(100.0)),
        };

        let result = process_entry(&mut engine, entry);
        assert!(result.is_ok());

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: 1,
            tx_id: 2,
            amount: Some(dec!(1.0)),
        };

        let result = process_entry(&mut engine, entry);
        assert!(result.is_ok());
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(101.0));

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Dispute,
            account_id: 1,
            tx_id: 2,
            amount: None,
        };

        let result = process_entry(&mut engine, entry.clone());
        assert!(result.is_ok(), "Dispute should be processed successfully");
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(1.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(101.0));
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(100.0));
        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&2).unwrap().status,
            TransactionStatus::Disputed
        );

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Chargeback,
            account_id: 1,
            tx_id: 2,
            amount: None,
        };
        let result = process_entry(&mut engine, entry.clone());
        assert!(
            result.is_ok(),
            "Chargeback should be processed successfully"
        );
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(100.0));
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(100.0));
        assert!(engine.accounts.get(&1).unwrap().locked);

        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&2).unwrap().status,
            TransactionStatus::Chargebacked
        );

        let result = process_entry(&mut engine, entry);
        assert!(result.is_err(), "Should not allow duplicate resolves");
    }

    #[test]
    fn process_dispute_for_absent_transactions() {
        let mut engine = PaymentEngine::new();

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: Some(dec!(100.0)),
        };

        process_entry(&mut engine, entry.clone()).unwrap();

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Dispute,
            account_id: 1,
            tx_id: 999,
            amount: None,
        };

        assert!(!engine.transactions.get(&1).unwrap().contains_key(&999));

        let result = process_entry(&mut engine, entry);
        assert!(
            result.is_err(),
            "Should return error for absent transactions"
        );
        assert!(!engine.transactions.get(&1).unwrap().contains_key(&999));

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Resolve,
            account_id: 1,
            tx_id: 999,
            amount: None,
        };

        let result = process_entry(&mut engine, entry);
        assert!(
            result.is_err(),
            "Should return error for absent transactions"
        );
        assert!(!engine.transactions.get(&1).unwrap().contains_key(&999));

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Chargeback,
            account_id: 1,
            tx_id: 999,
            amount: None,
        };

        let result = process_entry(&mut engine, entry);
        assert!(
            result.is_err(),
            "Should return error for absent transactions"
        );
        assert!(!engine.transactions.get(&1).unwrap().contains_key(&999));
    }

    #[test]
    fn test_dispute_with_incorrect_account_id() {
        let mut engine = PaymentEngine::new();

        let correct_account_id = 1;
        let incorrect_account_id = 2;
        let tx_id = 1;

        let entry = TransactionEntry {
            entry_type: TransactionEntryType::Deposit,
            account_id: correct_account_id,
            tx_id,
            amount: Some(dec!(100.0)),
        };

        let result = process_entry(&mut engine, entry);
        assert!(result.is_ok(), "Deposit should be processed successfully");
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().available,
            dec!(100.0)
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().total,
            dec!(100.0)
        );

        let incorrect_disput = TransactionEntry {
            entry_type: TransactionEntryType::Dispute,
            account_id: incorrect_account_id,
            tx_id,
            amount: None,
        };

        let result = process_entry(&mut engine, incorrect_disput);
        assert!(
            result.is_err(),
            "Dispute should fail when account_id doesn't match transaction's account"
        );

        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().available,
            dec!(100.0)
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().total,
            dec!(100.0)
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().held,
            dec!(0.0)
        );
        assert!(!engine.accounts.get(&correct_account_id).unwrap().locked);

        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&1).unwrap().status,
            TransactionStatus::Completed
        );

        assert!(!engine.accounts.contains_key(&incorrect_account_id));

        let correct_disput = TransactionEntry {
            entry_type: TransactionEntryType::Dispute,
            account_id: correct_account_id,
            tx_id,
            amount: None,
        };

        let result = process_entry(&mut engine, correct_disput);
        assert!(
            result.is_ok(),
            "Dispute should succeed with correct account_id"
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().available,
            dec!(0.0)
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().held,
            dec!(100.0)
        );
        assert_eq!(
            engine.accounts.get(&correct_account_id).unwrap().total,
            dec!(100.0)
        );
    }
}
