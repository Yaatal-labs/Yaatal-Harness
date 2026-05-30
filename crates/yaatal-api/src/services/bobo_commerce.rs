//! BOBO commerce persistence layer.
//!
//! Raw SQL through `sea_orm::Statement::from_sql_and_values` because the Lane 5b
//! migrations use Postgres-native types (PostGIS `geography`, `BIGSERIAL`, BYTEA,
//! TIMESTAMPTZ) that aren't expressible through the sea-orm schema builder. No
//! generated entities exist for these tables yet.
//!
//! Every function in this module short-circuits on a SQLite backend with a
//! `BackendUnsupported` error so in-memory tests don't pretend to work — the
//! BOBO surface is Postgres-only by design, documented in `KNOWN-ISSUES.md`.
//!
//! Escrow transitions delegate to `yaatal_core::commerce::escrow::transition`
//! so the state-machine table stays the single source of truth.

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, FromQueryResult, Statement, Value,
};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;
use yaatal_core::commerce::escrow::{self, EscrowError, EscrowState, EscrowTransition};

#[derive(Debug, Error)]
pub enum CommerceError {
    #[error("backend not supported (BOBO is Postgres-only)")]
    BackendUnsupported,
    #[error("not found")]
    NotFound,
    #[error("illegal order-state transition: cannot move {from} -> {to}")]
    IllegalOrderTransition { from: String, to: &'static str },
    #[error("escrow: {0}")]
    Escrow(#[from] EscrowError),
    #[error("db: {0}")]
    Db(#[from] DbErr),
    #[error("bad input: {0}")]
    BadInput(&'static str),
}

fn ensure_postgres(db: &DatabaseConnection) -> Result<(), CommerceError> {
    if db.get_database_backend() == DatabaseBackend::Postgres {
        Ok(())
    } else {
        Err(CommerceError::BackendUnsupported)
    }
}

fn parse_escrow_state(s: &str) -> Result<EscrowState, CommerceError> {
    match s {
        "held" => Ok(EscrowState::Held),
        "released" => Ok(EscrowState::Released),
        "settled" => Ok(EscrowState::Settled),
        "disputed" => Ok(EscrowState::Disputed),
        "refunded" => Ok(EscrowState::Refunded),
        _ => Err(CommerceError::BadInput("unknown escrow state in DB row")),
    }
}

// ─── Orders ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, FromQueryResult)]
pub struct OrderRow {
    pub id: i64,
    pub merchant_id: String,
    pub buyer_pid: Uuid,
    pub total_xof: i64,
    pub currency: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_order(
    db: &DatabaseConnection,
    merchant_id: String,
    buyer_pid: Uuid,
    total_xof: i64,
    delivery_lat: Option<f64>,
    delivery_lng: Option<f64>,
) -> Result<OrderRow, CommerceError> {
    ensure_postgres(db)?;
    if total_xof <= 0 {
        return Err(CommerceError::BadInput("total_xof must be > 0"));
    }

    let sql = r#"
        INSERT INTO bobo_orders (merchant_id, buyer_pid, total_xof, state, delivery_lat, delivery_lng)
        VALUES ($1, $2, $3, 'created', $4::float8, $5::float8)
        RETURNING id, merchant_id, buyer_pid, total_xof, currency, state, created_at, updated_at
    "#;
    let row = OrderRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            merchant_id.into(),
            buyer_pid.into(),
            total_xof.into(),
            delivery_lat.map(Value::from).unwrap_or(Value::Double(None)),
            delivery_lng.map(Value::from).unwrap_or(Value::Double(None)),
        ],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)?;
    Ok(row)
}

pub async fn list_orders_for_buyer(
    db: &DatabaseConnection,
    buyer_pid: Uuid,
    limit: u64,
) -> Result<Vec<OrderRow>, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        SELECT id, merchant_id, buyer_pid, total_xof, currency, state, created_at, updated_at
        FROM bobo_orders
        WHERE buyer_pid = $1
        ORDER BY created_at DESC
        LIMIT $2
    "#;
    let rows = OrderRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [buyer_pid.into(), (limit as i64).into()],
    ))
    .all(db)
    .await?;
    Ok(rows)
}

pub async fn get_order(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
) -> Result<OrderRow, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        SELECT id, merchant_id, buyer_pid, total_xof, currency, state, created_at, updated_at
        FROM bobo_orders
        WHERE id = $1 AND buyer_pid = $2
    "#;
    OrderRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [order_id.into(), buyer_pid.into()],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)
}

async fn update_order_state(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
    expected_from: &[&str],
    new_state: &'static str,
) -> Result<OrderRow, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        UPDATE bobo_orders
        SET state = $1, updated_at = now()
        WHERE id = $2
          AND buyer_pid = $3
          AND state = ANY($4::text[])
        RETURNING id, merchant_id, buyer_pid, total_xof, currency, state, created_at, updated_at
    "#;
    let expected_array: Vec<String> = expected_from.iter().map(|s| (*s).to_owned()).collect();
    OrderRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            new_state.into(),
            order_id.into(),
            buyer_pid.into(),
            expected_array.into(),
        ],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::IllegalOrderTransition {
        from: expected_from.join("|"),
        to: new_state,
    })
}

// ─── Escrow ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, FromQueryResult)]
pub struct EscrowRow {
    pub order_id: i64,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromQueryResult)]
pub struct PaymentIntentRow {
    pub order_id: i64,
    pub rail: String,
    pub provider_ref: String,
    pub idempotency_key: Uuid,
    pub status: String,
    pub amount_xof: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn get_escrow(
    db: &DatabaseConnection,
    order_id: i64,
) -> Result<Option<EscrowRow>, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        SELECT order_id, state, created_at, updated_at
        FROM bobo_escrow
        WHERE order_id = $1
    "#;
    let row = EscrowRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [order_id.into()],
    ))
    .one(db)
    .await?;
    Ok(row)
}

/// Insert a new escrow row in `held`. Used by the payment-settled webhook bridge
/// (and by the dogfood `simulate-payment` endpoint).
pub async fn create_escrow_held(
    db: &DatabaseConnection,
    order_id: i64,
) -> Result<EscrowRow, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        INSERT INTO bobo_escrow (order_id, state)
        VALUES ($1, 'held')
        ON CONFLICT (order_id) DO NOTHING
        RETURNING order_id, state, created_at, updated_at
    "#;
    EscrowRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [order_id.into()],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::IllegalOrderTransition {
        from: "(existing escrow)".into(),
        to: "held",
    })
}

pub async fn create_payment_intent(
    db: &DatabaseConnection,
    order_id: i64,
    rail: &str,
    provider_ref: &str,
    idempotency_key: Uuid,
    status: &str,
    amount_xof: i64,
) -> Result<PaymentIntentRow, CommerceError> {
    ensure_postgres(db)?;
    if amount_xof <= 0 {
        return Err(CommerceError::BadInput("amount_xof must be > 0"));
    }
    if !matches!(status, "pending" | "succeeded" | "failed" | "reversed") {
        return Err(CommerceError::BadInput("invalid payment intent status"));
    }

    let sql = r#"
        INSERT INTO bobo_payment_intents
            (order_id, rail, provider_ref, idempotency_key, status, amount_xof)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (idempotency_key) DO UPDATE
        SET updated_at = now()
        RETURNING order_id, rail, provider_ref, idempotency_key, status, amount_xof, created_at, updated_at
    "#;

    PaymentIntentRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            order_id.into(),
            rail.to_owned().into(),
            provider_ref.to_owned().into(),
            idempotency_key.into(),
            status.to_owned().into(),
            amount_xof.into(),
        ],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)
}

pub async fn get_payment_intent_for_order(
    db: &DatabaseConnection,
    order_id: i64,
) -> Result<PaymentIntentRow, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        SELECT order_id, rail, provider_ref, idempotency_key, status, amount_xof, created_at, updated_at
        FROM bobo_payment_intents
        WHERE order_id = $1
        ORDER BY created_at DESC
        LIMIT 1
    "#;

    PaymentIntentRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [order_id.into()],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)
}

/// Apply an escrow transition, gated by the pure state machine in
/// `yaatal_core::commerce::escrow::transition`. Uses optimistic UPDATE …
/// WHERE state = $expected so concurrent transitions can't smash each other.
pub async fn apply_escrow_transition(
    db: &DatabaseConnection,
    order_id: i64,
    ev: EscrowTransition,
) -> Result<EscrowRow, CommerceError> {
    ensure_postgres(db)?;
    let current_row = get_escrow(db, order_id)
        .await?
        .ok_or(CommerceError::NotFound)?;
    let current = parse_escrow_state(&current_row.state)?;
    let next = escrow::transition(current, ev)?;
    let sql = r#"
        UPDATE bobo_escrow
        SET state = $1, updated_at = now()
        WHERE order_id = $2 AND state = $3
        RETURNING order_id, state, created_at, updated_at
    "#;
    EscrowRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            next.to_string().into(),
            order_id.into(),
            current.to_string().into(),
        ],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::Escrow(EscrowError::IllegalTransition {
        from: current,
        transition: ev,
    }))
}

// ─── Order-level transitions composed with escrow ──────────────────────────────

/// Dogfood-only helper: simulate a successful payment landing. In production
/// this is invoked by the `payments` webhook bridge (Lane 5c, TODO).
pub async fn simulate_payment(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
) -> Result<(OrderRow, EscrowRow), CommerceError> {
    let order = update_order_state(db, order_id, buyer_pid, &["created"], "payment_held").await?;
    let escrow = create_escrow_held(db, order_id).await?;
    Ok((order, escrow))
}

pub async fn confirm_delivery(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
) -> Result<(OrderRow, EscrowRow), CommerceError> {
    let order = update_order_state(
        db,
        order_id,
        buyer_pid,
        &["payment_held"],
        "delivery_confirmed",
    )
    .await?;
    let escrow = apply_escrow_transition(db, order_id, EscrowTransition::Release).await?;
    Ok((order, escrow))
}

pub async fn dispute_order(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
) -> Result<(OrderRow, EscrowRow), CommerceError> {
    let order = update_order_state(
        db,
        order_id,
        buyer_pid,
        &["payment_held", "delivery_confirmed"],
        "disputed",
    )
    .await?;
    let escrow = apply_escrow_transition(db, order_id, EscrowTransition::Dispute).await?;
    Ok((order, escrow))
}

pub async fn cancel_order(
    db: &DatabaseConnection,
    order_id: i64,
    buyer_pid: Uuid,
) -> Result<OrderRow, CommerceError> {
    update_order_state(db, order_id, buyer_pid, &["created"], "cancelled").await
}

// ─── KYC ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, FromQueryResult)]
pub struct KycRow {
    pub pid: Uuid,
    pub status: String,
    pub provider: String,
    pub smile_id_ref: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub jurisdiction: String,
    pub created_at: DateTime<Utc>,
}

pub async fn submit_kyc(
    db: &DatabaseConnection,
    pid: Uuid,
    provider: &str,
    document_hash: Vec<u8>,
    jurisdiction: &str,
) -> Result<KycRow, CommerceError> {
    ensure_postgres(db)?;
    if document_hash.len() != 32 {
        return Err(CommerceError::BadInput(
            "document_hash must be 32 bytes (SHA-256 digest)",
        ));
    }
    // jurisdiction must be ISO-3166-1 alpha-2 — minimal sanity check.
    if jurisdiction.len() != 2 || !jurisdiction.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(CommerceError::BadInput(
            "jurisdiction must be 2 uppercase ASCII letters (ISO-3166-1 alpha-2)",
        ));
    }
    let sql = r#"
        INSERT INTO bobo_kyc (pid, status, provider, document_hash, jurisdiction)
        VALUES ($1, 'pending', $2, $3, $4)
        ON CONFLICT (pid) DO UPDATE
        SET status = 'pending', provider = EXCLUDED.provider,
            document_hash = EXCLUDED.document_hash, jurisdiction = EXCLUDED.jurisdiction
        RETURNING pid, status, provider, smile_id_ref, verified_at, jurisdiction, created_at
    "#;
    KycRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [
            pid.into(),
            provider.to_owned().into(),
            document_hash.into(),
            jurisdiction.to_owned().into(),
        ],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)
}

pub async fn get_kyc(db: &DatabaseConnection, pid: Uuid) -> Result<KycRow, CommerceError> {
    ensure_postgres(db)?;
    let sql = r#"
        SELECT pid, status, provider, smile_id_ref, verified_at, jurisdiction, created_at
        FROM bobo_kyc
        WHERE pid = $1
    "#;
    KycRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [pid.into()],
    ))
    .one(db)
    .await?
    .ok_or(CommerceError::NotFound)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_escrow_state_round_trip() {
        for s in ["held", "released", "settled", "disputed", "refunded"] {
            let parsed = parse_escrow_state(s).expect("known state");
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn parse_escrow_state_rejects_unknown() {
        assert!(matches!(
            parse_escrow_state("frozen"),
            Err(CommerceError::BadInput(_))
        ));
    }
}
