//! Nodes endpoint E2E tests.

use ram_api::models::NodeInfo;

use crate::e2e::TestClient;

/// Test that list_nodes returns at least one node.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_list_nodes_returns_local() {
    let client = TestClient::new();

    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to get nodes");

    assert!(!nodes.is_empty(), "Should return at least one node");

    // First node should be the local node and online
    let first_node = &nodes[0];
    assert!(first_node.online, "First node should be online");
}

/// Test that get_node returns a node by its actual ID.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_get_node_by_id() {
    let client = TestClient::new();

    // First get the list to find the actual node ID
    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to get nodes");

    assert!(!nodes.is_empty(), "Should have at least one node");

    let first_node = &nodes[0];
    let node_id = &first_node.id;

    // Now fetch by ID
    let node: NodeInfo = client
        .get_json(&format!("/nodes/{}", node_id))
        .await
        .expect("Failed to get node by ID");

    assert_eq!(node.id, *node_id);
    assert!(node.online, "Node should be online");
}

/// Test that get_node returns 404 for nonexistent node.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_get_nonexistent_node_returns_404() {
    let client = TestClient::new();

    let response = client
        .get("/nodes/nonexistent-node-12345")
        .await
        .expect("Failed to send request");

    assert_eq!(response.status().as_u16(), 404);
}
