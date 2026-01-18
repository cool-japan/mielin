# MielinOS CLI Benchmarks

This directory contains performance benchmarks for critical operations in the MielinCTL CLI.

## Running Benchmarks

Run all benchmarks:
```bash
cargo bench
```

Run specific benchmark group:
```bash
cargo bench config
cargo bench output_formatting
cargo bench agent_operations
cargo bench error_handling
cargo bench json_operations
cargo bench table_formatting
cargo bench operation_result
```

Run a specific benchmark:
```bash
cargo bench --bench cli_benchmarks -- create_default_config
```

## Benchmark Groups

### 1. Configuration Operations (`bench_config_operations`)
- `create_default_config` - Creating default configuration
- `serialize_config_toml` - Serializing config to TOML
- `deserialize_config_toml` - Deserializing TOML to config
- `get_config_value` - Reading configuration values
- `set_config_value` - Writing configuration values

### 2. Output Formatting (`bench_output_formatting`)
- `format_single_node_json` - JSON formatting for single node
- `format_single_node_yaml` - YAML formatting for single node
- `format_10_nodes_json` - JSON formatting for 10 nodes
- `format_10_nodes_yaml` - YAML formatting for 10 nodes
- `format_10_nodes_json_pretty` - Pretty JSON formatting for 10 nodes

### 3. Agent Operations (`bench_agent_operations`)
- `serialize_agent_json` - Serializing single agent to JSON
- `serialize_100_agents_json` - Serializing 100 agents to JSON
- `filter_agents_by_state` - Filtering agents by state
- `filter_agents_by_node` - Filtering agents by node

### 4. Error Handling (`bench_error_handling`)
- `create_cli_error` - Creating error instances
- `format_error_message` - Formatting error messages
- `create_and_format_error` - Combined error creation and formatting

### 5. JSON Operations (`bench_json_operations`)
- `parse_small_json` - Parsing small JSON documents
- `parse_large_json` - Parsing large JSON documents
- `stringify_small_json` - Stringifying small JSON
- `stringify_large_json` - Stringifying large JSON
- `pretty_print_large_json` - Pretty printing large JSON

### 6. Table Formatting (`bench_table_formatting`)
- `format_50_nodes_table` - Formatting 50 nodes as a table

### 7. Operation Results (`bench_operation_result`)
- `create_operation_result` - Creating operation result instances
- `serialize_operation_result` - Serializing operation results to JSON

## Viewing Results

Benchmark results are saved in `target/criterion/`:
```bash
# View HTML report
open target/criterion/report/index.html

# View specific benchmark
open target/criterion/config/create_default_config/report/index.html
```

## Performance Targets

Expected performance (approximate, hardware-dependent):

| Operation | Target | Notes |
|-----------|--------|-------|
| create_default_config | < 1 µs | Should be nearly instant |
| serialize_config_toml | < 10 µs | TOML serialization |
| deserialize_config_toml | < 20 µs | TOML parsing |
| get_config_value | < 500 ns | Map lookup |
| set_config_value | < 2 µs | Map update + validation |
| format_single_node_json | < 5 µs | JSON serialization |
| format_10_nodes_json | < 50 µs | Batch JSON serialization |
| serialize_100_agents_json | < 500 µs | Large JSON serialization |
| filter_agents_by_state | < 10 µs | Iterator filtering |
| parse_small_json | < 5 µs | Small JSON parsing |
| parse_large_json | < 500 µs | Large JSON parsing |
| format_50_nodes_table | < 1 ms | Complex table rendering |

## Comparing Performance

Compare against saved baseline:
```bash
# Save current performance as baseline
cargo bench -- --save-baseline main

# Run benchmarks and compare
cargo bench -- --baseline main
```

## Tips for Optimization

1. **Configuration**: Keep config operations fast - they're called frequently
2. **Output Formatting**: JSON is fastest, tables are slower but needed for UX
3. **Filtering**: Consider caching filtered results for repeated operations
4. **Large Data**: Be mindful of O(n) operations on large collections
5. **Error Handling**: Keep error creation and formatting cheap

## Continuous Integration

These benchmarks can be run in CI to detect performance regressions:
```bash
# Run benchmarks with baseline comparison
cargo bench -- --baseline ci-baseline
```

## Adding New Benchmarks

To add a new benchmark:
1. Create a new function following the pattern `bench_feature_name(c: &mut Criterion)`
2. Add it to the `criterion_group!` macro at the end of the file
3. Document it in this README

Example:
```rust
fn bench_my_feature(c: &mut Criterion) {
    let mut group = c.benchmark_group("my_feature");

    group.bench_function("operation_name", |b| {
        b.iter(|| {
            // Your code to benchmark
            black_box(my_operation());
        });
    });

    group.finish();
}

// Add to criterion_group!
criterion_group!(
    benches,
    // ... existing benchmarks
    bench_my_feature,
);
```

## Resources

- [Criterion.rs Documentation](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
