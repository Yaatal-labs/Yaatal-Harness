//! `yaatal-payments` — internal payment wrapper for YAATAL.
//!
//! Scope-lock (from Notion *Payment wrapper* TASK.md §1):
//! this crate wraps payment methods on rails where NJOOBA already holds (or will
//! open) a merchant account. Money settles into YAATAL/NJOOBA-owned accounts.
//! YAATAL never takes custody of third-party funds, never routes money between
//! unrelated parties, and is not an aggregator or payment service provider for
//! external merchants. Any change that would cross into holding or routing
//! other people's money is out of scope and must be flagged.
