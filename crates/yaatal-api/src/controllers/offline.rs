#![allow(clippy::unused_async)]
use axum::{extract::State, response::IntoResponse, routing::post, Form};
use loco_rs::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UssdRequest {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "phoneNumber")]
    pub phone_number: String,
    #[serde(rename = "networkCode")]
    pub network_code: String,
    #[serde(rename = "serviceCode")]
    pub service_code: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct SmsRequest {
    pub from: String,
    pub to: String,
    pub date: String,
    pub text: String,
    pub id: String,
}

/// Handles incoming USSD traffic from Africa's Talking.
/// Always returns specialized plaintext strings prefixed with `CON ` or `END `
pub async fn ussd_webhook(
    State(_ctx): State<AppContext>,
    Form(payload): Form<UssdRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "Received USSD from {}: {}",
        payload.phone_number,
        payload.text
    );

    let response = if payload.text.is_empty() {
        // Initial dial.
        "CON Welcome to Yaatal Engine\n1. Top Headlines\n2. Post Update\n3. Check XP"
    } else if payload.text == "1" {
        // Here we would query the SeaORM database for the latest Feed posts.
        "END Headlines: \n1: 'AI breaks limits.'\n2: 'Dakar tech boom.'"
    } else if payload.text == "3" {
        // Match user by phone number from DB and fetch XP.
        "END You currently have 1500 XP. Keep engaging!"
    } else {
        "END Invalid input. Please try again."
    };

    // USSD responses must be purely plain text
    (axum::http::StatusCode::OK, response)
}

/// Handles incoming SMS messages.
pub async fn sms_webhook(
    State(_ctx): State<AppContext>,
    Form(payload): Form<SmsRequest>,
) -> Result<Response> {
    tracing::info!(
        "Received SMS offline post from {}: {}",
        payload.from,
        payload.text
    );
    // TODO(#E7): Save SMS content as a Post in the database via yaatal-core.
    format::empty()
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/offline")
        .add("/ussd", post(ussd_webhook))
        .add("/sms", post(sms_webhook))
}
