//! E2E Test Entry Point
//!
//! Run all E2E tests with:
//! ```bash
//! cargo test -p ram-service --test e2e_tests -- --ignored
//! ```
//!
//! These tests require a running AudioMatrix service at localhost:8080.

mod e2e;
