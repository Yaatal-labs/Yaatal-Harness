/// Order API response views.
use serde::{Deserialize, Serialize};

/// Response for an order item.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderItemResponse {
    pub id: String,
    pub product_id: String,
    pub quantity: i32,
    pub unit_price_cents: i32,
}

/// Response for a single order.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub buyer_id: String,
    pub seller_id: String,
    pub status: String,
    pub payment_method: String,
    pub payment_status: String,
    pub delivery_method: String,
    pub total_cents: i32,
    pub items: Vec<OrderItemResponse>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Paginated list of orders.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderListResponse {
    pub orders: Vec<OrderResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

/// Response for merchant dashboard stats.
#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardResponse {
    pub total_revenue_cents: i64,
    pub total_orders: i64,
    pub orders_by_status: std::collections::HashMap<String, i64>,
}
