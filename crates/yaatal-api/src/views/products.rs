/// Product API response views.
use serde::{Deserialize, Serialize};

/// Response for a single product.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProductResponse {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price_cents: i32,
    pub discount_price_cents: Option<i32>,
    pub stock: i32,
    pub category: String,
    pub images: Option<String>,
    pub is_active: bool,
    pub upvotes: i32,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Paginated list of products.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProductListResponse {
    pub products: Vec<ProductResponse>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}
