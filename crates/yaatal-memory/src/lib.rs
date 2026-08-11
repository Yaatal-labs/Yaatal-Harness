//! Memory system for the Yaatal AI harness.
//!
//! This crate provides memory storage implementations for agent context.
//! Based on research from Anthropic and MiniMax:
//!
//! - **Anthropic**: Session memory via progress files, feature lists
//! - **MiniMax**: Short-term (per-round), session, persistent memory
//!
//! ## Memory Tiers
//!
//! | Tier | Duration | Use Case |
//! |------|----------|----------|
//! | Short-term | Within context | Tool results, current thinking |
//! | Session | Across sessions | Progress, feature lists |
//! | Persistent | Across projects | User preferences, learned facts |
//!
//! ## Example
//!
//! ```rust
//! use yaatal_memory::InMemoryStore;
//! // `store`/`recall` are trait methods, so `MemoryStore` has to be in scope to call
//! // them — and both it and `MemoryEntry` live in `yaatal-core`, not here.
//! use yaatal_core::{MemoryEntry, MemoryStore};
//!
//! #[tokio::main]
//! async fn main() {
//!     let store = InMemoryStore::new();
//!
//!     // Store a memory
//!     let entry = MemoryEntry::new("fact", "The project uses Rust");
//!     let id = store.store(entry).await.unwrap();
//!
//!     // Recall it
//!     let recalled = store.recall(&id).await.unwrap();
//!     println!("{:?}", recalled);
//! }
//! ```

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;
use yaatal_core::{MemoryEntry, MemoryError, MemoryStore, MemoryType};

// =============================================================================
// IN-MEMORY STORE (Simple implementation)
// =============================================================================

/// Simple in-memory implementation of MemoryStore.
/// Suitable for testing and single-instance deployments.
/// For production, consider SqliteStore or RedisStore.
pub struct InMemoryStore {
    entries: RwLock<HashMap<String, MemoryEntry>>,
    /// Index by type for faster filtering
    by_type: RwLock<HashMap<MemoryType, Vec<String>>>,
}

impl InMemoryStore {
    /// Create a new in-memory store.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            by_type: RwLock::new(HashMap::new()),
        }
    }

    /// Get the total number of entries.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Check if empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryStore for InMemoryStore {
    async fn store(&self, mut entry: MemoryEntry) -> Result<String, MemoryError> {
        // Generate ID if not set
        if entry.id.is_empty() {
            entry.id = Uuid::new_v4().to_string();
        }

        // Set timestamps
        let now = Utc::now();
        entry.created_at = now;
        entry.accessed_at = now;

        // Store the entry
        let id = entry.id.clone();
        self.entries.write().await.insert(id.clone(), entry.clone());

        // Update type index
        self.by_type
            .write()
            .await
            .entry(entry.memory_type.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        tracing::debug!(memory_id = %id, memory_type = ?entry.memory_type, "Memory stored");
        Ok(id)
    }

    async fn recall(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let mut entries = self.entries.write().await;

        if let Some(entry) = entries.get_mut(id) {
            // Update access time
            entry.accessed_at = Utc::now();
            tracing::debug!(memory_id = %id, "Memory recalled");
            Ok(Some(entry.clone()))
        } else {
            Ok(None)
        }
    }

    async fn search(
        &self,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let entries = self.entries.read().await;
        let query_lower = query.to_lowercase();

        let mut results: Vec<MemoryEntry> = entries
            .values()
            .filter(|e| {
                // Filter by type if specified
                if let Some(ref t) = memory_type {
                    if &e.memory_type != t {
                        return false;
                    }
                }
                // Search in content
                e.content.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        // Sort by accessed_at descending (most recent first)
        results.sort_by(|a, b| b.accessed_at.cmp(&a.accessed_at));

        // Apply limit
        results.truncate(limit);

        tracing::debug!(query = %query, results = results.len(), "Memory search completed");
        Ok(results)
    }

    async fn recent(
        &self,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let entries = self.entries.read().await;

        let mut results: Vec<MemoryEntry> = entries
            .values()
            .filter(|e| {
                if let Some(ref t) = memory_type {
                    e.memory_type == *t
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Sort by accessed_at descending
        results.sort_by(|a, b| b.accessed_at.cmp(&a.accessed_at));
        results.truncate(limit);

        Ok(results)
    }

    async fn forget(&self, id: &str) -> Result<(), MemoryError> {
        let entry = self.entries.write().await.remove(id);

        if let Some(e) = entry {
            // Remove from type index
            if let Some(ids) = self.by_type.write().await.get_mut(&e.memory_type) {
                ids.retain(|i| i != id);
            }
            tracing::debug!(memory_id = %id, "Memory forgotten");
        }

        Ok(())
    }

    async fn update(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        let id = entry.id.clone();

        if !self.entries.read().await.contains_key(&id) {
            return Err(MemoryError::NotFound(id));
        }

        let mut entries = self.entries.write().await;
        let old_entry = entries.get(&id).cloned();

        if let Some(old) = old_entry {
            // Update type index if type changed
            if old.memory_type != entry.memory_type {
                let mut by_type = self.by_type.write().await;

                // Remove from old type
                if let Some(ids) = by_type.get_mut(&old.memory_type) {
                    ids.retain(|i| i != &id);
                }

                // Add to new type
                by_type
                    .entry(entry.memory_type.clone())
                    .or_insert_with(Vec::new)
                    .push(id.clone());
            }
        }

        let mut updated = entry;
        updated.accessed_at = Utc::now();
        entries.insert(id.clone(), updated);

        tracing::debug!(memory_id = %id, "Memory updated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_recall() {
        let store = InMemoryStore::new();
        let entry = MemoryEntry::fact("Rust is awesome");
        let id = store.store(entry).await.unwrap();

        let recalled = store.recall(&id).await.unwrap();
        assert!(recalled.is_some());
        assert_eq!(recalled.unwrap().content, "Rust is awesome");
    }

    #[tokio::test]
    async fn test_search() {
        let store = InMemoryStore::new();
        store
            .store(MemoryEntry::fact("Apple is a fruit"))
            .await
            .unwrap();
        store
            .store(MemoryEntry::fact("Car is a vehicle"))
            .await
            .unwrap();
        store
            .store(MemoryEntry::project("Build a car"))
            .await
            .unwrap();

        let results = store.search("car", None, 10).await.unwrap();
        assert_eq!(results.len(), 2); // fact and project both mention "car"
    }

    #[tokio::test]
    async fn test_filter_by_type() {
        let store = InMemoryStore::new();
        store.store(MemoryEntry::fact("Fact 1")).await.unwrap();
        store.store(MemoryEntry::fact("Fact 2")).await.unwrap();
        store
            .store(MemoryEntry::project("Project 1"))
            .await
            .unwrap();

        let facts = store.recent(Some(MemoryType::Fact), 10).await.unwrap();
        assert_eq!(facts.len(), 2);

        let projects = store.recent(Some(MemoryType::Project), 10).await.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn test_forget() {
        let store = InMemoryStore::new();
        let entry = MemoryEntry::fact("To be forgotten");
        let id = store.store(entry).await.unwrap();

        store.forget(&id).await.unwrap();
        let recalled = store.recall(&id).await.unwrap();
        assert!(recalled.is_none());
    }
}
