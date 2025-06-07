# Transaction Processing Engine

A financial transaction processing engine written in Rust that simulates real-world payment processing. The system manages client accounts and handles various transaction types including deposits, withdrawals, disputes, resolutions, and chargebacks.

## Overview

This engine processes financial transactions from CSV input and maintains accurate account balances with proper dispute resolution mechanisms. It ensures data integrity through precise decimal arithmetic and comprehensive error handling.

## Requirements

- **Input**: CSV file containing transaction records
- **Processing**: Support for 5 transaction types with full business logic
- **Output**: Account status report in CSV format with 4-digit precision

## Key Design Decisions

### Financial Precision
- Uses `rust_decimal` instead of floating-point types to prevent rounding errors
- Maintains 4-digit precision throughout all calculations
- Ensures accurate financial computations for production use

### CSV Format Handling
- Processes unquoted CSV with flexible spacing
- Handles optional amount field for dispute-related transactions
- Robust parsing with error reporting for malformed records

### Account States
- **Open**: Normal account allowing all transaction types
- **Locked**: Restricted account (post-chargeback) rejecting new transactions
- All operations on locked accounts are automatically rejected

### Balance Management
- **Available**: Funds accessible for withdrawals
- **Held**: Funds temporarily frozen due to disputes
- **Total**: Sum of available and held funds
- Prevents negative balances through pre-transaction validation

## Architecture

The system is built around several core components:

### PaymentEngine
The main engine that orchestrates all transaction processing and account management.

## Transaction Types

The engine supports five types of financial transactions:

### Basic Transactions
- **Deposit**: Adds funds to a client account
  - Increases both available and total balance
  - Creates a new account if it doesn't exist
  - Always marked as "Completed" status

- **Withdrawal**: Removes funds from a client account
  - Decreases both available and total balance
  - Requires sufficient available funds
  - Fails if account has insufficient balance

### Dispute Resolution
- **Dispute**: Initiates a dispute for a deposit transaction
  - Moves funds from available to held balance
  - Only valid for completed deposit transactions
  - Changes transaction status to "Disputed"

- **Resolve**: Resolves a dispute in favor of the client
  - Moves funds from held back to available balance
  - Only valid for disputed transactions
  - Changes transaction status to "Resolved"

- **Chargeback**: Resolves a dispute against the client
  - Removes held funds from the account entirely
  - Locks the account permanently
  - Changes transaction status to "Chargebacked"

## Transaction States

Transactions flow through the following states:
- **Completed**: Initial state for successful transactions
- **Disputed**: Transaction is under dispute (funds held)
- **Resolved**: Dispute resolved in favor of the client
- **Chargebacked**: Dispute resolved against the client (account locked)

## Error Handling

The system provides comprehensive error handling through the `PaymentError` enum:

- `AccountNotFound`: Requested account doesn't exist
- `AccountLocked`: Account is locked due to chargeback
- `TransactionNotFound`: Transaction doesn't exist
- `TransactionAlreadyExists`: Duplicate transaction ID
- `InsufficientFunds`: Not enough available balance for withdrawal
- `InsufficientHoldFunds`: Not enough held funds for dispute resolution
- `InvalidTransactionType`: Operation not valid for transaction type
- `TransactionAlreadyDisputed`: Transaction is already disputed/resolved/chargebacked
- `TransactionIsNotDisputed`: Trying to resolve/chargeback non-disputed transaction

## Data Structures

### Account
```rust
pub struct Account {
    pub client: u16,
    pub available: Decimal,  // Available balance for withdrawals
    pub held: Decimal,       // Funds held due to disputes
    pub total: Decimal,      // Total balance (available + held)
    pub locked: bool,        // Account locked due to chargeback
}
```

### Transaction
```rust
pub struct Transaction {
    pub tx_type: TransactionType,
    pub account_id: u16,
    pub tx_id: u32,
    pub amount: Decimal,
    pub status: TransactionStatus,
}
```

## Safety and Reliability

### Balance Integrity
- All balance modifications are atomic and validated
- Separate tracking of available, held, and total balances
- Precise decimal arithmetic prevents rounding errors
- Prevents negative balances through pre-transaction validation

### State Management
- Transactions can only transition through valid states
- Duplicate transaction IDs are rejected
- Disputes with incorrect account IDs are rejected
- Account locking prevents further operations after chargebacks

### Thread Safety
The current implementation is not thread-safe. For concurrent usage, additional synchronization mechanisms would be required.

For concurrent usage, HashMap should be replaced with DashMap or SCC to allow usage between threads with minimal overhead. For asynchronous code we can use asynchronous HashMap implementations like SCC with asynchronous sync primitives like Tokio.

To prevent double charging when the same transaction arrives simultaneously, we need to make changes to both account and transaction data transactionally. In this case, the Account data structure should include all Transactions. We can acquire a lock and perform both checks and updates within the same locking window.

```rust
pub struct Account {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
    pub transactions: DashMap<u32, Transaction>
}

pub type Accounts = DashMap<u16, Account>;
```

We ensure that all data is passed by reference only to avoid copying the entire transaction list. Alternatively, we can include transactions as a separate reference with the same lifetime as Accounts.

## Building and Testing

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run with optimizations
cargo build --release
```

## Dependencies

- `rust_decimal`: For precise decimal arithmetic
- `csv`: For CSV parsing and processing
- `serde`: For serialization/deserialization

## CSV Input Format

Expected CSV format:
```csv
type, client, tx, amount
deposit, 1, 1, 100.0
withdrawal, 1, 2, 50.0
dispute, 1, 1,
resolve, 1, 1,
chargeback, 1, 1,
```

Note: Dispute, resolve, and chargeback transactions don't require an amount field.

## CSV Output Format

The engine outputs account status in CSV format:

```csv
client, available, held, total, locked
1, 100.0000, 0.0000, 100.0000, false
2, 50.0000, 25.0000, 75.0000, false
```

All monetary values are displayed with 4-digit precision.
