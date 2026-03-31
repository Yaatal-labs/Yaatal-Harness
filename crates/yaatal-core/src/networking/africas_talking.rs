use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info};

#[derive(Debug, Error)]
pub enum AfricasTalkingError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("API error returned status {0}: {1}")]
    ApiError(StatusCode, String),
    #[error("Configuration error: missing API key or username")]
    ConfigError,
    #[error("HTTP client build error: {0}")]
    ClientBuild(String),
}

#[derive(Clone)]
pub struct AfricasTalkingClient {
    client: Client,
    username: String,
    api_key: String,
    is_sandbox: bool,
}

#[derive(Serialize)]
struct SmsPayload<'a> {
    username: &'a str,
    to: &'a str,
    message: &'a str,
}

#[derive(Deserialize, Debug)]
pub struct SmsResponse {
    #[serde(rename = "SMSMessageData")]
    pub data: Option<SmsMessageData>,
}

#[derive(Deserialize, Debug)]
pub struct SmsMessageData {
    #[serde(rename = "Message")]
    pub message: String,
    #[serde(rename = "Recipients")]
    pub recipients: Vec<SmsRecipient>,
}

#[derive(Deserialize, Debug)]
pub struct SmsRecipient {
    #[serde(rename = "statusCode")]
    pub status_code: u16,
    pub number: String,
    pub status: String,
    pub cost: String,
    #[serde(rename = "messageId")]
    pub message_id: String,
}

impl AfricasTalkingClient {
    pub fn new(
        username: String,
        api_key: String,
        is_sandbox: bool,
    ) -> Result<Self, AfricasTalkingError> {
        if username.trim().is_empty() || api_key.trim().is_empty() {
            return Err(AfricasTalkingError::ConfigError);
        }

        let client = Client::builder()
            .build()
            .map_err(|err| AfricasTalkingError::ClientBuild(err.to_string()))?;

        Ok(Self {
            client,
            username,
            api_key,
            is_sandbox,
        })
    }

    fn base_url(&self) -> &'static str {
        if self.is_sandbox {
            "https://api.sandbox.africastalking.com/version1"
        } else {
            "https://api.africastalking.com/version1"
        }
    }

    /// Sends an SMS via the Africa's Talking API
    pub async fn send_sms(
        &self,
        to: &str,
        message: &str,
    ) -> Result<SmsResponse, AfricasTalkingError> {
        let url = format!("{}/messaging", self.base_url());

        let payload = SmsPayload {
            username: &self.username,
            to,
            message,
        };

        info!("Sending AT SMS to {}", to);

        let response = self
            .client
            .post(&url)
            .header("ApiKey", &self.api_key)
            .header("Accept", "application/json")
            .form(&payload)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!("Africa's Talking API Error ({}): {}", status, error_text);
            return Err(AfricasTalkingError::ApiError(status, error_text));
        }

        let parsed: SmsResponse = response.json().await?;
        Ok(parsed)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_blank_credentials() {
        let err = AfricasTalkingClient::new(" ".to_string(), "key".to_string(), true)
            .expect_err("blank username must fail");
        assert!(matches!(err, AfricasTalkingError::ConfigError));

        let err = AfricasTalkingClient::new("user".to_string(), "".to_string(), true)
            .expect_err("blank api key must fail");
        assert!(matches!(err, AfricasTalkingError::ConfigError));
    }

    #[test]
    fn sms_response_deserializes_with_snake_case_fields() {
        let payload = r#"{
            "SMSMessageData": {
                "Message": "Sent to 1/1 Total Cost: KES 0.8000",
                "Recipients": [
                    {
                        "statusCode": 101,
                        "number": "+254711000000",
                        "status": "Success",
                        "cost": "KES 0.8000",
                        "messageId": "ATXid_123"
                    }
                ]
            }
        }"#;

        let parsed: SmsResponse = serde_json::from_str(payload).expect("valid sms response json");
        let data = parsed.data.expect("sms message data should be present");
        assert_eq!(data.message, "Sent to 1/1 Total Cost: KES 0.8000");
        assert_eq!(data.recipients.len(), 1);
        assert_eq!(data.recipients[0].status_code, 101);
        assert_eq!(data.recipients[0].message_id, "ATXid_123");
    }
}
