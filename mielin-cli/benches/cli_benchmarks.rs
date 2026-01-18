use criterion::{criterion_group, criterion_main, Criterion};
use mielin_cli::config::Config;
use mielin_cli::error::CliError;
use mielin_cli::{AgentInfo, NodeInfo, OperationResult};
use serde_json::Value;
use std::hint::black_box;

/// Benchmark configuration loading and parsing
fn bench_config_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("config");

    group.bench_function("create_default_config", |b| {
        b.iter(|| {
            black_box(Config::default());
        });
    });

    group.bench_function("serialize_config_toml", |b| {
        let config = Config::default();
        b.iter(|| {
            black_box(toml::to_string(&config).unwrap());
        });
    });

    group.bench_function("deserialize_config_toml", |b| {
        let config_str = toml::to_string(&Config::default()).unwrap();
        b.iter(|| {
            black_box(toml::from_str::<Config>(&config_str).unwrap());
        });
    });

    group.bench_function("get_config_value", |b| {
        let config = Config::default();
        b.iter(|| {
            black_box(config.get("node.id"));
        });
    });

    group.bench_function("set_config_value", |b| {
        b.iter(|| {
            let mut config = Config::default();
            config.set("node.id", "bench-node").unwrap();
            black_box(());
        });
    });

    group.finish();
}

/// Benchmark output formatting for different formats
fn bench_output_formatting(c: &mut Criterion) {
    let mut group = c.benchmark_group("output_formatting");

    // Create sample data
    let node_info = NodeInfo {
        id: "node-001".to_string(),
        role: "core".to_string(),
        state: "Running".to_string(),
        address: "0.0.0.0:7000".to_string(),
        agents: 10,
        uptime: "1h".to_string(),
        version: "0.1.0".to_string(),
    };

    let nodes: Vec<NodeInfo> = (0..10)
        .map(|i| NodeInfo {
            id: format!("node-{:03}", i),
            role: if i % 3 == 0 {
                "core"
            } else if i % 3 == 1 {
                "relay"
            } else {
                "edge"
            }
            .to_string(),
            state: "Running".to_string(),
            address: format!("0.0.0.0:700{}", i),
            agents: i * 5,
            uptime: format!("{}h", i + 1),
            version: "0.1.0".to_string(),
        })
        .collect();

    group.bench_function("format_single_node_json", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&node_info).unwrap());
        });
    });

    group.bench_function("format_single_node_yaml", |b| {
        b.iter(|| {
            black_box(serde_yaml::to_string(&node_info).unwrap());
        });
    });

    group.bench_function("format_10_nodes_json", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&nodes).unwrap());
        });
    });

    group.bench_function("format_10_nodes_yaml", |b| {
        b.iter(|| {
            black_box(serde_yaml::to_string(&nodes).unwrap());
        });
    });

    // Benchmark pretty printing
    group.bench_function("format_10_nodes_json_pretty", |b| {
        b.iter(|| {
            black_box(serde_json::to_string_pretty(&nodes).unwrap());
        });
    });

    group.finish();
}

/// Benchmark agent operations
fn bench_agent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_operations");

    let agent_info = AgentInfo {
        id: "agent-001-xxxxxxxxxx".to_string(),
        state: "Running".to_string(),
        node: "node-001".to_string(),
        dna_hash: "abc123def456".to_string(),
        memory_mb: 128.5,
        uptime: "30m".to_string(),
    };

    let agents: Vec<AgentInfo> = (0..100)
        .map(|i| AgentInfo {
            id: format!("agent-{:03}-xxxxxxxxxx", i),
            state: if i % 5 == 0 { "Running" } else { "Paused" }.to_string(),
            node: format!("node-{:03}", i % 10),
            dna_hash: format!("hash{:010}", i),
            memory_mb: 64.0 + (i as f64) * 2.0,
            uptime: format!("{}m", i),
        })
        .collect();

    group.bench_function("serialize_agent_json", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&agent_info).unwrap());
        });
    });

    group.bench_function("serialize_100_agents_json", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&agents).unwrap());
        });
    });

    group.bench_function("filter_agents_by_state", |b| {
        b.iter(|| {
            let filtered: Vec<_> = agents.iter().filter(|a| a.state == "Running").collect();
            black_box(filtered);
        });
    });

    group.bench_function("filter_agents_by_node", |b| {
        b.iter(|| {
            let filtered: Vec<_> = agents.iter().filter(|a| a.node == "node-001").collect();
            black_box(filtered);
        });
    });

    group.finish();
}

/// Benchmark error handling
fn bench_error_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_handling");

    group.bench_function("create_cli_error", |b| {
        b.iter(|| {
            black_box(CliError::Connection("Connection failed".to_string()));
        });
    });

    group.bench_function("format_error_message", |b| {
        let error = CliError::Connection("Connection failed".to_string());
        b.iter(|| {
            black_box(format!("{}", error));
        });
    });

    group.bench_function("create_and_format_error", |b| {
        b.iter(|| {
            let error = CliError::Connection("Connection failed".to_string());
            black_box(format!("{}", error));
        });
    });

    group.finish();
}

/// Benchmark JSON parsing and manipulation
fn bench_json_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_operations");

    let small_json = r#"{"id": "node-001", "state": "Running"}"#;
    let large_json = serde_json::to_string(&vec![
        serde_json::json!({
            "id": format!("node-{:03}", 0),
            "role": "core",
            "state": "Running",
            "agents": (0..50).map(|i| format!("agent-{:03}", i)).collect::<Vec<_>>()
        });
        100
    ])
    .unwrap();

    group.bench_function("parse_small_json", |b| {
        b.iter(|| {
            black_box(serde_json::from_str::<Value>(small_json).unwrap());
        });
    });

    group.bench_function("parse_large_json", |b| {
        b.iter(|| {
            black_box(serde_json::from_str::<Value>(&large_json).unwrap());
        });
    });

    group.bench_function("stringify_small_json", |b| {
        let value = serde_json::from_str::<Value>(small_json).unwrap();
        b.iter(|| {
            black_box(serde_json::to_string(&value).unwrap());
        });
    });

    group.bench_function("stringify_large_json", |b| {
        let value = serde_json::from_str::<Value>(&large_json).unwrap();
        b.iter(|| {
            black_box(serde_json::to_string(&value).unwrap());
        });
    });

    group.bench_function("pretty_print_large_json", |b| {
        let value = serde_json::from_str::<Value>(&large_json).unwrap();
        b.iter(|| {
            black_box(serde_json::to_string_pretty(&value).unwrap());
        });
    });

    group.finish();
}

/// Benchmark table formatting
fn bench_table_formatting(c: &mut Criterion) {
    let mut group = c.benchmark_group("table_formatting");

    let nodes: Vec<NodeInfo> = (0..50)
        .map(|i| NodeInfo {
            id: format!("node-{:03}", i),
            role: if i % 3 == 0 {
                "core"
            } else if i % 3 == 1 {
                "relay"
            } else {
                "edge"
            }
            .to_string(),
            state: "Running".to_string(),
            address: format!("0.0.0.0:700{}", i),
            agents: i * 5,
            uptime: format!("{}h", i + 1),
            version: "0.1.0".to_string(),
        })
        .collect();

    group.bench_function("format_50_nodes_table", |b| {
        b.iter(|| {
            use tabled::{Table, Tabled};
            #[derive(Tabled)]
            struct NodeRow {
                id: String,
                role: String,
                state: String,
                address: String,
                agents: String,
            }

            let rows: Vec<NodeRow> = nodes
                .iter()
                .map(|n| NodeRow {
                    id: n.id.clone(),
                    role: n.role.clone(),
                    state: n.state.clone(),
                    address: n.address.clone(),
                    agents: n.agents.to_string(),
                })
                .collect();

            black_box(Table::new(rows).to_string());
        });
    });

    group.finish();
}

/// Benchmark operation result tracking
fn bench_operation_result(c: &mut Criterion) {
    let mut group = c.benchmark_group("operation_result");

    group.bench_function("create_operation_result", |b| {
        b.iter(|| {
            black_box(OperationResult {
                success: true,
                message: "Deploying agent".to_string(),
                id: Some("op-001".to_string()),
            });
        });
    });

    group.bench_function("serialize_operation_result", |b| {
        let result = OperationResult {
            success: true,
            message: "Deploying agent".to_string(),
            id: Some("op-001".to_string()),
        };
        b.iter(|| {
            black_box(serde_json::to_string(&result).unwrap());
        });
    });

    group.finish();
}

/// Benchmark plugin operations
fn bench_plugin_operations(c: &mut Criterion) {
    use mielin_cli::{PluginCommand, PluginManager, PluginMetadata};

    let mut group = c.benchmark_group("plugin");

    group.bench_function("create_plugin_manager", |b| {
        b.iter(|| {
            black_box(PluginManager::new().unwrap());
        });
    });

    group.bench_function("serialize_plugin_metadata", |b| {
        let metadata = PluginMetadata {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            commands: vec![PluginCommand {
                name: "hello".to_string(),
                description: "Say hello".to_string(),
                aliases: vec!["hi".to_string()],
                arguments: vec![],
            }],
            dependencies: vec![],
            min_version: Some("0.1.0".to_string()),
        };

        b.iter(|| {
            black_box(toml::to_string(&metadata).unwrap());
        });
    });

    group.finish();
}

/// Benchmark script operations
fn bench_script_operations(c: &mut Criterion) {
    use mielin_cli::{ScriptEngine, ScriptMetadata};

    let mut group = c.benchmark_group("script");

    group.bench_function("create_script_engine", |b| {
        b.iter(|| {
            black_box(ScriptEngine::new().unwrap());
        });
    });

    group.bench_function("serialize_script_metadata", |b| {
        let metadata = ScriptMetadata {
            name: "test-script".to_string(),
            version: "1.0.0".to_string(),
            description: "A test script".to_string(),
            author: Some("Test Author".to_string()),
            required_version: Some("0.1.0".to_string()),
            tags: vec!["test".to_string(), "automation".to_string()],
        };

        b.iter(|| {
            black_box(serde_json::to_string(&metadata).unwrap());
        });
    });

    group.finish();
}

/// Benchmark remote management operations
fn bench_remote_operations(c: &mut Criterion) {
    use mielin_cli::{AuthMethod, ConnectionOptions, RemoteManager, RemoteNode};

    let mut group = c.benchmark_group("remote");

    group.bench_function("create_remote_manager", |b| {
        b.iter(|| {
            black_box(RemoteManager::new().unwrap());
        });
    });

    group.bench_function("serialize_remote_node", |b| {
        let node = RemoteNode {
            id: "node1".to_string(),
            name: "Test Node".to_string(),
            address: "localhost:8080".to_string(),
            auth: AuthMethod::None,
            options: ConnectionOptions::default(),
            tags: vec!["test".to_string(), "dev".to_string()],
            description: "A test node".to_string(),
        };

        b.iter(|| {
            black_box(toml::to_string(&node).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_config_operations,
    bench_output_formatting,
    bench_agent_operations,
    bench_error_handling,
    bench_json_operations,
    bench_table_formatting,
    bench_operation_result,
    bench_plugin_operations,
    bench_script_operations,
    bench_remote_operations,
);

criterion_main!(benches);
