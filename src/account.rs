use rust_decimal::Decimal;

pub struct Account {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

pub type Accounts = std::collections::HashMap<u16, Account>;
