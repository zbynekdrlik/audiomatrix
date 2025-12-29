//! Nodes endpoint E2E tests.

use ram_api::models::NodeInfo;

use crate::e2e::TestClient;

/// Test that list_nodes returns at least the local node.
#[tokio::test]
#[ignore] // Requires running server
async fn test_list_nodes_returns_local() {
    let client = TestClient::new();

    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to get nodes");

    assert!(!nodes.is_empty(), "Should return at least one node");

    // Find local node (LOCAL is the default ID for local node)
    let local_node = nodes.iter().find(|n| n.id == "LOCAL");
    assert!(local_node.is_some(), "Should have a local node");
}

/// Test that get_node returns the local node.
#[tokio::test]
#[ignore] // Requires running server
async fn test_get_node_local() {
    let client = TestClient::new();

    let node: NodeInfo = client
        .get_json("/nodes/LOCAL")
        .await
        .expect("Failed to get local node");

    assert!(node.online, "Local node should be online");
}

/// Test that get_node returns 404 for nonexistent node.
#[tokio::test]
#[ignore] // Requires running server
async fn test_get_nonexistent_node_returns_404() {
    let client = TestClient::new();

    let response = client
        .get("/nodes/nonexistent-node-12345")
        .await
        .expect("Failed to send request");

    assert_eq!(response.status().as_u16(), 404);
}
