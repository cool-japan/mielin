//! Integration tests for mielinctl CLI
//!
//! These tests verify the complete command-line interface functionality.

use assert_cmd::Command;
use predicates::prelude::*;

/// Helper function to create a command for the mielinctl binary
#[allow(deprecated)]
fn mielinctl() -> Command {
    Command::cargo_bin("mielinctl").expect("Failed to find mielinctl binary")
}

/// Creates a temporary directory for test isolation and returns both the dir and a
/// helper that creates `mielinctl` subprocesses with HOME/XDG_CONFIG_HOME redirected
/// to that temp dir, preventing shared-config conflicts under parallel test execution.
struct IsolatedEnv {
    home: std::path::PathBuf,
}

impl IsolatedEnv {
    fn new() -> Self {
        use std::time::SystemTime;
        let suffix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time drift")
            .subsec_nanos();
        let pid = std::process::id();
        let home = std::env::temp_dir().join(format!("mielin-test-{}-{}", pid, suffix));
        std::fs::create_dir_all(&home).expect("create isolated home dir");
        IsolatedEnv { home }
    }

    /// Create a mielinctl Command with HOME and XDG_CONFIG_HOME set to the isolated dir
    fn cmd(&self) -> Command {
        let mut cmd = mielinctl();
        cmd.env("HOME", &self.home);
        cmd.env("XDG_CONFIG_HOME", self.home.join(".config"));
        cmd
    }
}

impl Drop for IsolatedEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[test]
fn test_version_command() {
    let mut cmd = mielinctl();
    cmd.arg("version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MielinOS CLI"));
}

#[test]
fn test_version_json_output() {
    let mut cmd = mielinctl();
    cmd.arg("version").arg("--output").arg("json");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"version\""));
}

#[test]
fn test_help_command() {
    let mut cmd = mielinctl();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("MielinOS Control Interface"));
}

#[test]
fn test_node_list() {
    let mut cmd = mielinctl();
    cmd.arg("node").arg("list");
    cmd.assert().success();
}

#[test]
fn test_node_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("node").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_node_list_json_output() {
    let mut cmd = mielinctl();
    cmd.arg("node").arg("list").arg("--output").arg("json");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"nodes\""));
}

#[test]
fn test_node_info() {
    let mut cmd = mielinctl();
    cmd.arg("node").arg("info").arg("test-node-id");
    cmd.assert().success();
}

#[test]
fn test_node_info_alias_show() {
    let mut cmd = mielinctl();
    cmd.arg("node").arg("show").arg("test-node-id");
    cmd.assert().success();
}

#[test]
fn test_agent_list() {
    // `agent list` still succeeds, but it serves sample/mock data (there is
    // no live daemon connection wired up here) — the CLI must say so clearly
    // rather than presenting it as real state.
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("list");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("sample/mock"));
}

#[test]
fn test_agent_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("ls");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("sample/mock"));
}

#[test]
fn test_agent_list_with_filter() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("list").arg("--state").arg("Running");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("sample/mock"));
}

#[test]
fn test_agent_inspect() {
    // `agent inspect` still succeeds, but it must clearly label its output
    // as sample/mock data rather than fabricating real agent details.
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("inspect").arg("test-agent-id");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("sample/mock"))
        .stdout(predicate::str::contains("sample/mock"));
}

#[test]
fn test_agent_inspect_alias_show() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("show").arg("test-agent-id");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("sample/mock"))
        .stdout(predicate::str::contains("sample/mock"));
}

#[test]
fn test_agent_deploy_requires_live_daemon() {
    // `agent deploy` must not fabricate a success/UUID without ever talking
    // to a daemon: it should fail with an explicit, honest error instead.
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("deploy").arg("some.wasm");
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );
}

#[test]
fn test_agent_migrate_requires_live_daemon() {
    let mut cmd = mielinctl();
    cmd.arg("agent")
        .arg("migrate")
        .arg("test-agent-id")
        .arg("test-target-node");
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );
}

#[test]
fn test_agent_stop_requires_live_daemon() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("stop").arg("test-agent-id");
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );
}

#[test]
fn test_agent_stop_alias_kill() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("kill").arg("test-agent-id");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires a live daemon"));
}

#[test]
fn test_agent_logs_requires_live_daemon() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("logs").arg("test-agent-id");
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );
}

#[test]
fn test_agent_create_without_wasm_file() {
    let mut cmd = mielinctl();
    cmd.arg("agent")
        .arg("create")
        .arg("nonexistent.wasm")
        .arg("--name")
        .arg("test-agent");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("WASM file not found"));
}

#[test]
fn test_agent_create_invalid_env_format() {
    let mut cmd = mielinctl();

    // Create a temporary wasm file for testing
    let temp_dir = std::env::temp_dir();
    let wasm_path = temp_dir.join("test.wasm");
    std::fs::write(&wasm_path, b"test").unwrap();

    cmd.arg("agent")
        .arg("create")
        .arg(&wasm_path)
        .arg("--name")
        .arg("test-agent")
        .arg("--env")
        .arg("INVALID_FORMAT");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Invalid environment variable format",
    ));

    // Cleanup
    std::fs::remove_file(&wasm_path).ok();
}

#[test]
fn test_agent_create_valid_input_requires_live_daemon() {
    // Even with fully valid input, `agent create` must not fabricate a
    // successfully-created agent id: there is no live daemon to actually
    // create it against, so it must fail honestly after validation passes.
    let mut cmd = mielinctl();

    let temp_dir = std::env::temp_dir();
    let wasm_path = temp_dir.join("test_agent_create_valid.wasm");
    std::fs::write(&wasm_path, b"test").unwrap();

    cmd.arg("agent")
        .arg("create")
        .arg(&wasm_path)
        .arg("--name")
        .arg("test-agent");

    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );

    std::fs::remove_file(&wasm_path).ok();
}

#[test]
fn test_agent_exec_no_command() {
    let mut cmd = mielinctl();
    cmd.arg("agent").arg("exec").arg("test-agent-id");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No command specified"));
}

#[test]
fn test_agent_exec_requires_live_daemon() {
    // A valid command must not be reported as "executed successfully"
    // without ever reaching a live agent runtime.
    let mut cmd = mielinctl();
    cmd.arg("agent")
        .arg("exec")
        .arg("test-agent-id")
        .arg("--")
        .arg("echo")
        .arg("hello");
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("requires a live daemon").and(predicate::str::contains(
                "not yet supported over the control plane",
            )),
        );
}

#[test]
fn test_cluster_init() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("init").arg("test-cluster");
    cmd.assert().success();
}

#[test]
fn test_cluster_init_alias_setup() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("setup").arg("test-cluster");
    cmd.assert().success();
}

#[test]
fn test_cluster_status() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("status");
    cmd.assert().success();
}

#[test]
fn test_cluster_status_alias_st() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("st");
    cmd.assert().success();
}

#[test]
fn test_cluster_health() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("health");
    cmd.assert().success();
}

#[test]
fn test_cluster_health_alias_check() {
    let mut cmd = mielinctl();
    cmd.arg("cluster").arg("check");
    cmd.assert().success();
}

#[test]
fn test_mesh_status() {
    let mut cmd = mielinctl();
    cmd.arg("mesh").arg("status");
    cmd.assert().success();
}

#[test]
fn test_mesh_status_alias_st() {
    let mut cmd = mielinctl();
    cmd.arg("mesh").arg("st");
    cmd.assert().success();
}

#[test]
fn test_mesh_peers() {
    let mut cmd = mielinctl();
    cmd.arg("mesh").arg("peers");
    cmd.assert().success();
}

#[test]
fn test_mesh_peers_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("mesh").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_migrate_status() {
    let mut cmd = mielinctl();
    cmd.arg("migrate").arg("status");
    cmd.assert().success();
}

#[test]
fn test_migrate_status_alias_st() {
    let mut cmd = mielinctl();
    cmd.arg("migrate").arg("st");
    cmd.assert().success();
}

#[test]
fn test_migrate_history() {
    let mut cmd = mielinctl();
    cmd.arg("migrate").arg("history");
    cmd.assert().success();
}

#[test]
fn test_migrate_history_with_limit() {
    let mut cmd = mielinctl();
    cmd.arg("migrate").arg("history").arg("-n").arg("5");
    cmd.assert().success();
}

#[test]
fn test_registry_list() {
    let mut cmd = mielinctl();
    cmd.arg("registry").arg("list");
    cmd.assert().success();
}

#[test]
fn test_registry_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("registry").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_registry_query() {
    let mut cmd = mielinctl();
    cmd.arg("registry").arg("query").arg("test-pattern");
    cmd.assert().success();
}

#[test]
fn test_registry_query_alias_search() {
    let mut cmd = mielinctl();
    cmd.arg("registry").arg("search").arg("test-pattern");
    cmd.assert().success();
}

#[test]
fn test_registry_stats() {
    let mut cmd = mielinctl();
    cmd.arg("registry").arg("stats");
    cmd.assert().success();
}

#[test]
fn test_gossip_status() {
    let mut cmd = mielinctl();
    cmd.arg("gossip").arg("status");
    cmd.assert().success();
}

#[test]
fn test_gossip_members() {
    let mut cmd = mielinctl();
    cmd.arg("gossip").arg("members");
    cmd.assert().success();
}

#[test]
fn test_gossip_members_alive_filter() {
    let mut cmd = mielinctl();
    cmd.arg("gossip").arg("members").arg("--alive");
    cmd.assert().success();
}

#[test]
fn test_gossip_sync() {
    let mut cmd = mielinctl();
    cmd.arg("gossip").arg("sync");
    cmd.assert().success();
}

#[test]
fn test_wasm_validate_nonexistent() {
    let mut cmd = mielinctl();
    cmd.arg("wasm").arg("validate").arg("nonexistent.wasm");
    cmd.assert().failure();
}

#[test]
fn test_wasm_validate_alias_check() {
    let mut cmd = mielinctl();

    // Create a temporary wasm file for testing
    let temp_dir = std::env::temp_dir();
    let wasm_path = temp_dir.join("test_validate.wasm");
    std::fs::write(&wasm_path, b"test").unwrap();

    cmd.arg("wasm").arg("check").arg(&wasm_path);
    cmd.assert().success();

    // Cleanup
    std::fs::remove_file(&wasm_path).ok();
}

#[test]
fn test_debug_attach() {
    let mut cmd = mielinctl();
    cmd.arg("debug").arg("attach").arg("test-agent-id");
    cmd.assert().success();
}

#[test]
fn test_debug_attach_with_port() {
    let mut cmd = mielinctl();
    cmd.arg("debug")
        .arg("attach")
        .arg("test-agent-id")
        .arg("--port")
        .arg("9999");
    cmd.assert().success();
}

#[test]
fn test_debug_trace() {
    let mut cmd = mielinctl();
    cmd.arg("debug").arg("trace").arg("test-agent-id");
    cmd.assert().success();
}

#[test]
fn test_debug_dump() {
    let mut cmd = mielinctl();
    cmd.arg("debug").arg("dump").arg("test-agent-id");
    cmd.assert().success();
}

#[test]
fn test_debug_profile() {
    let mut cmd = mielinctl();
    cmd.arg("debug").arg("profile").arg("test-agent-id");
    cmd.assert().success();
}

#[test]
fn test_quiet_mode() {
    let mut cmd = mielinctl();
    cmd.arg("--quiet").arg("node").arg("list");
    cmd.assert().success();
}

#[test]
fn test_output_format_yaml() {
    let mut cmd = mielinctl();
    cmd.arg("--output").arg("yaml").arg("node").arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("nodes:"));
}

#[test]
fn test_completion_bash() {
    let mut cmd = mielinctl();
    cmd.arg("completion").arg("bash");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completion_zsh() {
    let mut cmd = mielinctl();
    cmd.arg("completion").arg("zsh");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("compdef"));
}

// Plugin command tests
#[test]
fn test_plugin_list() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("list");
    cmd.assert().success();
}

#[test]
fn test_plugin_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_plugin_list_alias_plugins() {
    let mut cmd = mielinctl();
    cmd.arg("plugins").arg("list");
    cmd.assert().success();
}

#[test]
fn test_plugin_list_json() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("list").arg("--output").arg("json");
    cmd.assert().success();
}

#[test]
fn test_plugin_directory() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("directory");
    cmd.assert().success();
}

#[test]
fn test_plugin_directory_alias_dir() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("dir");
    cmd.assert().success();
}

#[test]
fn test_plugin_reload() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("reload");
    cmd.assert().success();
}

#[test]
fn test_plugin_info_not_found() {
    let mut cmd = mielinctl();
    cmd.arg("plugin").arg("info").arg("nonexistent-plugin");
    cmd.assert().failure();
}

// Script command tests
#[test]
fn test_script_list() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("list");
    cmd.assert().success();
}

#[test]
fn test_script_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_script_list_alias_scripts() {
    let mut cmd = mielinctl();
    cmd.arg("scripts").arg("list");
    cmd.assert().success();
}

#[test]
fn test_script_list_with_tag() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("list").arg("--tag").arg("automation");
    cmd.assert().success();
}

#[test]
fn test_script_list_json() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("list").arg("--output").arg("json");
    cmd.assert().success();
}

#[test]
fn test_script_directory() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("directory");
    cmd.assert().success();
}

#[test]
fn test_script_directory_alias_dir() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("dir");
    cmd.assert().success();
}

#[test]
fn test_script_reload() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("reload");
    cmd.assert().success();
}

#[test]
fn test_script_info_not_found() {
    let mut cmd = mielinctl();
    cmd.arg("script").arg("info").arg("nonexistent-script");
    cmd.assert().failure();
}

#[test]
fn test_script_create_template() {
    use std::env;
    let temp_file = env::temp_dir().join("test_script.rhai");

    let mut cmd = mielinctl();
    cmd.arg("script")
        .arg("create")
        .arg("test-script")
        .arg("--file")
        .arg(&temp_file);
    cmd.assert().success();

    // Clean up
    let _ = std::fs::remove_file(&temp_file);
}

// Remote command tests
#[test]
fn test_remote_list() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("list");
    cmd.assert().success();
}

#[test]
fn test_remote_list_alias_ls() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("ls");
    cmd.assert().success();
}

#[test]
fn test_remote_list_alias_nodes() {
    let mut cmd = mielinctl();
    cmd.arg("nodes").arg("list");
    cmd.assert().success();
}

#[test]
fn test_remote_list_with_tag() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("list").arg("--tag").arg("production");
    cmd.assert().success();
}

#[test]
fn test_remote_list_json() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("list").arg("--output").arg("json");
    cmd.assert().success();
}

#[test]
fn test_remote_config() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("config");
    cmd.assert().success();
}

#[test]
fn test_remote_config_alias_path() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("path");
    cmd.assert().success();
}

#[test]
fn test_remote_info_not_found() {
    let mut cmd = mielinctl();
    cmd.arg("remote").arg("info").arg("nonexistent-node");
    cmd.assert().failure();
}

#[test]
fn test_remote_add_remove() {
    let node_id = format!("test-node-{}", std::process::id());
    let env = IsolatedEnv::new();

    let mut cmd = env.cmd();
    cmd.arg("remote")
        .arg("add")
        .arg("--id")
        .arg(&node_id)
        .arg("--name")
        .arg("Test Node")
        .arg("--address")
        .arg("localhost:8080")
        .arg("--auth")
        .arg("none");
    cmd.assert().success();

    // Remove the node
    let mut cmd = env.cmd();
    cmd.arg("remote").arg("remove").arg(&node_id).arg("-y");
    cmd.assert().success();
}

#[test]
fn test_remote_add_with_tags() {
    let node_id = format!("test-node-tags-{}", std::process::id());
    let env = IsolatedEnv::new();

    let mut cmd = env.cmd();
    cmd.arg("remote")
        .arg("add")
        .arg("--id")
        .arg(&node_id)
        .arg("--name")
        .arg("Tagged Node")
        .arg("--address")
        .arg("localhost:8081")
        .arg("--auth")
        .arg("none")
        .arg("--tags")
        .arg("test,ci");
    cmd.assert().success();

    // Clean up
    let mut cmd = env.cmd();
    cmd.arg("remote").arg("remove").arg(&node_id).arg("-y");
    cmd.assert().success();
}

// Multi-format output tests for new commands
#[test]
fn test_plugin_list_yaml() {
    let mut cmd = mielinctl();
    cmd.arg("--output").arg("yaml").arg("plugin").arg("list");
    cmd.assert().success();
}

#[test]
fn test_script_list_yaml() {
    let mut cmd = mielinctl();
    cmd.arg("--output").arg("yaml").arg("script").arg("list");
    cmd.assert().success();
}

#[test]
fn test_remote_list_yaml() {
    let mut cmd = mielinctl();
    cmd.arg("--output").arg("yaml").arg("remote").arg("list");
    cmd.assert().success();
}

// Quiet mode tests for new commands
#[test]
fn test_plugin_list_quiet() {
    let mut cmd = mielinctl();
    cmd.arg("--quiet").arg("plugin").arg("list");
    cmd.assert().success();
}

#[test]
fn test_script_list_quiet() {
    let mut cmd = mielinctl();
    cmd.arg("--quiet").arg("script").arg("list");
    cmd.assert().success();
}

#[test]
fn test_remote_list_quiet() {
    let mut cmd = mielinctl();
    cmd.arg("--quiet").arg("remote").arg("list");
    cmd.assert().success();
}
