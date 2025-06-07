use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;

use rust_decimal::Decimal;
use rust_decimal::dec;

use crate::account::Account;
use crate::error::PaymentError;
use crate::transaction::Transaction;
use crate::transaction::TransactionStatus;
use crate::transaction::TransactionType;

pub type Accounts = HashMap<u16, Account>;
pub type AccountTransactions = HashMap<u32, Transaction>;
pub type Transactions = HashMap<u16, HashMap<u32, Transaction>>;

pub struct PaymentEngine {
    pub accounts: Accounts,
    pub transactions: Transactions,
}

impl Default for PaymentEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PaymentEngine {
    pub fn new() -> Self {
        PaymentEngine {
            accounts: Accounts::new(),
            transactions: Transactions::new(),
        }
    }

    #[inline]
    fn update_account_balance(
        &mut self,
        account_id: u16,
        available_delta: Decimal,
        held_delta: Decimal,
        total_delta: Decimal,
    ) -> Result<(), PaymentError> {
        if let Some(account) = self.accounts.get_mut(&account_id) {
            if (account.available + available_delta) < dec!(0)
                || (account.held + held_delta) < dec!(0)
                || (account.total + total_delta) < dec!(0)
            {
                return Err(PaymentError::InsufficientFunds);
            }
            account.available += available_delta;
            account.held += held_delta;
            account.total += total_delta;
            Ok(())
        } else {
            Err(PaymentError::AccountNotFound(account_id))
        }
    }

    #[inline]
    fn update_transaction_status(
        &mut self,
        account_id: u16,
        tx_id: u32,
        new_status: TransactionStatus,
    ) -> Result<(), PaymentError> {
        let account_transactions = self
            .transactions
            .get_mut(&account_id)
            .ok_or(PaymentError::TransactionNotFound)?;

        if let Some(existing_transaction) = account_transactions.get_mut(&tx_id) {
            existing_transaction.status = new_status;
            Ok(())
        } else {
            Err(PaymentError::TransactionNotFound)
        }
    }

    #[inline]
    pub fn get_deposit_transaction_status(
        &self,
        account_id: u16,
        tx_id: u32,
    ) -> Result<&Transaction, PaymentError> {
        let account_transactions = self
            .transactions
            .get(&account_id)
            .ok_or(PaymentError::TransactionNotFound)?;

        if let Some(transaction) = account_transactions.get(&tx_id) {
            if transaction.tx_type != TransactionType::Deposit {
                return Err(PaymentError::InvalidTransactionType);
            }
            Ok(transaction)
        } else {
            Err(PaymentError::TransactionNotFound)
        }
    }

    #[inline]
    fn check_transaction(&self, account_id: u16, tx_id: u32) -> bool {
        self.transactions
            .get(&account_id)
            .and_then(|a| a.get(&tx_id))
            .is_some()
    }

    #[inline]
    fn get_or_create_account(&mut self, account_id: u16) -> &Account {
        (self.accounts.entry(account_id).or_insert(Account {
            client: account_id,
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            total: Decimal::ZERO,
            locked: false,
        })) as _
    }

    #[inline]
    fn insert_transaction(&mut self, transaction: Transaction) {
        let account_transactions = self.transactions.entry(transaction.account_id).or_default();
        account_transactions.insert(transaction.tx_id, transaction);
    }

    #[inline]
    fn lock_account(&mut self, account_id: u16) {
        if let Some(account) = self.accounts.get_mut(&account_id) {
            account.locked = true;
        }
    }

    #[inline]
    fn is_account_locked(&self, account_id: u16) -> bool {
        self.accounts
            .get(&account_id)
            .map(|a| a.locked)
            .unwrap_or(false)
    }

    pub fn process_transaction(&mut self, transaction: Transaction) -> Result<(), PaymentError> {
        let account = self.get_or_create_account(transaction.account_id);

        let account_available = account.available;

        if self.is_account_locked(transaction.account_id) {
            return Err(PaymentError::AccountLocked(transaction.account_id));
        }

        if self.check_transaction(transaction.account_id, transaction.tx_id) {
            return Err(PaymentError::TransactionAlreadyExists);
        }

        let (available_delta, held_delta, total_delta) = match transaction.tx_type {
            TransactionType::Deposit => (transaction.amount, Decimal::ZERO, transaction.amount),
            TransactionType::Withdrawal => {
                if account_available >= transaction.amount {
                    (-transaction.amount, Decimal::ZERO, -transaction.amount)
                } else {
                    return Err(PaymentError::InsufficientFunds);
                }
            }
        };

        self.update_account_balance(
            transaction.account_id,
            available_delta,
            held_delta,
            total_delta,
        )?;
        self.insert_transaction(transaction);
        Ok(())
    }

    pub fn process_dispute(&mut self, account_id: u16, tx_id: u32) -> Result<(), PaymentError> {
        if self.is_account_locked(account_id) {
            return Err(PaymentError::AccountLocked(account_id));
        }

        let existing_transaction = self.get_deposit_transaction_status(account_id, tx_id)?;
        if existing_transaction.status == TransactionStatus::Completed {
            let amount = existing_transaction.amount;
            if let Some(account) = self.accounts.get(&account_id) {
                if account.available < amount {
                    return Err(PaymentError::InsufficientHoldFunds);
                }
            } else {
                return Err(PaymentError::AccountNotFound(account_id));
            }

            self.update_account_balance(account_id, -amount, amount, Decimal::ZERO)?;
            self.update_transaction_status(account_id, tx_id, TransactionStatus::Disputed)?;
            Ok(())
        } else {
            Err(PaymentError::TransactionAlreadyDisputed)
        }
    }

    pub fn process_resolve(&mut self, account_id: u16, tx_id: u32) -> Result<(), PaymentError> {
        if self.is_account_locked(account_id) {
            return Err(PaymentError::AccountLocked(account_id));
        }

        let existing_transaction = self.get_deposit_transaction_status(account_id, tx_id)?;

        if existing_transaction.status != TransactionStatus::Disputed {
            if existing_transaction.status == TransactionStatus::Resolved
                || existing_transaction.status == TransactionStatus::Chargebacked
            {
                return Err(PaymentError::TransactionAlreadyDisputed);
            } else {
                return Err(PaymentError::TransactionIsNotDisputed);
            }
        }

        let amount = existing_transaction.amount;

        if let Some(account) = self.accounts.get(&account_id) {
            if account.held < amount {
                return Err(PaymentError::InsufficientHoldFunds);
            }
        } else {
            return Err(PaymentError::AccountNotFound(account_id));
        }

        self.update_account_balance(account_id, amount, -amount, Decimal::ZERO)?;
        self.update_transaction_status(account_id, tx_id, TransactionStatus::Resolved)?;
        Ok(())
    }

    pub fn process_chargeback(&mut self, account_id: u16, tx_id: u32) -> Result<(), PaymentError> {
        if self.is_account_locked(account_id) {
            return Err(PaymentError::AccountLocked(account_id));
        }

        let existing_transaction = self.get_deposit_transaction_status(account_id, tx_id)?;

        if existing_transaction.status != TransactionStatus::Disputed {
            if existing_transaction.status == TransactionStatus::Resolved
                || existing_transaction.status == TransactionStatus::Chargebacked
            {
                return Err(PaymentError::TransactionAlreadyDisputed);
            } else {
                return Err(PaymentError::TransactionIsNotDisputed);
            }
        }

        let amount = existing_transaction.amount;

        if let Some(account) = self.accounts.get(&account_id) {
            if account.held < amount {
                return Err(PaymentError::InsufficientHoldFunds);
            }
        } else {
            return Err(PaymentError::AccountNotFound(account_id));
        }

        self.update_account_balance(account_id, Decimal::ZERO, -amount, -amount)?;
        self.update_transaction_status(account_id, tx_id, TransactionStatus::Chargebacked)?;
        self.lock_account(account_id);
        Ok(())
    }
}

impl Display for PaymentEngine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "client, available, held, total, locked")?;

        for account in self.accounts.values() {
            writeln!(
                f,
                "{}, {:.4}, {:.4}, {:.4}, {}",
                account.client, account.available, account.held, account.total, account.locked
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn test_payment_engine_display() {
        let mut engine = PaymentEngine::new();

        engine.accounts.insert(
            1,
            Account {
                client: 1,
                available: dec!(100.1234),
                held: dec!(50.5678),
                total: dec!(150.6912),
                locked: false,
            },
        );

        engine.accounts.insert(
            2,
            Account {
                client: 2,
                available: dec!(0.0),
                held: dec!(25.0),
                total: dec!(25.0),
                locked: true,
            },
        );

        engine.accounts.insert(
            3,
            Account {
                client: 3,
                available: dec!(999.9999),
                held: dec!(0.0001),
                total: dec!(1000.0),
                locked: false,
            },
        );

        let output = format!("{}", engine);

        assert!(output.contains("client, available, held, total, locked"));
        assert!(output.contains("1, 100.1234, 50.5678, 150.6912, false"));
        assert!(output.contains("2, 0.0000, 25.0000, 25.0000, true"));
        assert!(output.contains("3, 999.9999, 0.0001, 1000.0000, false"));

        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 4);

        for line in &lines[1..] {
            let values: Vec<&str> = line.split(", ").collect();
            assert_eq!(values.len(), 5);
        }
    }

    #[test]
    fn test_payment_engine_display_empty() {
        let engine = PaymentEngine::new();
        let output = format!("{}", engine);
        assert_eq!(output.trim(), "client, available, held, total, locked");
    }

    #[test]
    fn test_withdrawal_insufficient_funds() {
        let mut engine = PaymentEngine::new();

        let deposit = Transaction {
            tx_type: TransactionType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: dec!(50.0),
            status: TransactionStatus::Completed,
        };

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(50.0), dec!(0.0), dec!(50.0))
            .unwrap();
        engine.insert_transaction(deposit);

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(50.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(50.0));

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(100.0),
            status: TransactionStatus::Completed,
        };

        let should_fail = engine.process_transaction(withdrawal);
        assert!(should_fail.is_err(), "Should detect insufficient funds");

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(50.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(50.0));
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));
    }

    #[test]
    fn test_dispute_insufficient_available_balance() {
        let mut engine = PaymentEngine::new();

        let deposit = Transaction {
            tx_type: TransactionType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
            status: TransactionStatus::Completed,
        };

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(100.0), dec!(0.0), dec!(100.0))
            .unwrap();
        engine.insert_transaction(deposit);

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(80.0),
            status: TransactionStatus::Completed,
        };
        engine
            .process_transaction(withdrawal)
            .expect("Withdrawal should succeed");

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(20.0));

        let result = engine.process_dispute(1, 1);
        assert!(
            result.is_err(),
            "Dispute should fail due to insufficient available funds"
        );

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));

        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&1).unwrap().status,
            TransactionStatus::Completed
        );
    }

    #[test]
    fn test_withdrawal_exact_balance() {
        let mut engine = PaymentEngine::new();

        let deposit = Transaction {
            tx_type: TransactionType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: dec!(50.0),
            status: TransactionStatus::Completed,
        };

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(50.0), dec!(0.0), dec!(50.0))
            .unwrap();
        engine.insert_transaction(deposit);

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
            status: TransactionStatus::Completed,
        };
        engine
            .process_transaction(withdrawal)
            .expect("Withdrawal should succeed");

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(0.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(0.0));
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));
    }

    #[test]
    fn test_withdrawal_process_with_insufficient_funds() {
        let mut engine = PaymentEngine::new();

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(50.0), dec!(0.0), dec!(50.0))
            .unwrap();

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(100.0),
            status: TransactionStatus::Completed,
        };

        let result = engine.process_transaction(withdrawal);

        assert!(
            result.is_err(),
            "Should not have sufficient funds for withdrawal"
        );
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(50.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(50.0));
    }

    #[test]
    fn test_dispute_process_with_insufficient_available_balance() {
        let mut engine = PaymentEngine::new();

        let deposit = Transaction {
            tx_type: TransactionType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
            status: TransactionStatus::Completed,
        };

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(100.0), dec!(0.0), dec!(100.0))
            .unwrap();
        engine.insert_transaction(deposit);

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(80.0),
            status: TransactionStatus::Completed,
        };
        engine
            .process_transaction(withdrawal)
            .expect("Withdrawal should succeed");

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(20.0));

        let result = engine.process_dispute(1, 1);

        assert!(
            result.is_err(),
            "Should not have sufficient available balance for dispute"
        );
        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(0.0));

        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&1).unwrap().status,
            TransactionStatus::Completed
        );
    }

    #[test]
    fn test_successful_dispute_after_partial_withdrawal() {
        let mut engine = PaymentEngine::new();

        let deposit = Transaction {
            tx_type: TransactionType::Deposit,
            account_id: 1,
            tx_id: 1,
            amount: dec!(30.0),
            status: TransactionStatus::Completed,
        };

        engine.get_or_create_account(1);
        engine
            .update_account_balance(1, dec!(100.0), dec!(0.0), dec!(100.0))
            .unwrap();
        engine.insert_transaction(deposit);

        let withdrawal = Transaction {
            tx_type: TransactionType::Withdrawal,
            account_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
            status: TransactionStatus::Completed,
        };

        engine
            .process_transaction(withdrawal)
            .expect("Withdrawal should succeed");

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(50.0));

        let result = engine.process_dispute(1, 1);
        assert!(
            result.is_ok(),
            "Dispute should succeed when sufficient available balance"
        );

        assert_eq!(engine.accounts.get(&1).unwrap().available, dec!(20.0));
        assert_eq!(engine.accounts.get(&1).unwrap().held, dec!(30.0));
        assert_eq!(engine.accounts.get(&1).unwrap().total, dec!(50.0));

        assert_eq!(
            engine.transactions.get(&1).unwrap().get(&1).unwrap().status,
            TransactionStatus::Disputed
        );
    }
}
