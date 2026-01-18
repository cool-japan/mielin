//! Remote Monitoring Example
//!
//! This example demonstrates:
//! - System health monitoring
//! - Performance metrics collection
//! - Alert configuration and triggering
//! - Time-series data management
//! - Remote diagnostics setup

#![no_std]
#![no_main]

extern crate alloc;
extern crate panic_halt;

use core::alloc::Layout;

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

struct DummyAllocator;

unsafe impl core::alloc::GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

use mielin_rt::monitoring::{
    AlertCondition, AlertDefinition, AlertSeverity, HealthStatus, MetricMetadata, MetricType,
    MetricValue, RemoteMonitor,
};

static mut SYSTEM_TIME_MS: u32 = 0;

fn system_time_ms() -> u32 {
    unsafe { SYSTEM_TIME_MS }
}

fn advance_time(ms: u32) {
    unsafe {
        SYSTEM_TIME_MS += ms;
    }
}

// Metric IDs
const METRIC_CPU_USAGE: u32 = 1;
const METRIC_MEMORY_USAGE: u32 = 2;
const METRIC_TEMPERATURE: u32 = 3;
const METRIC_BATTERY_VOLTAGE: u32 = 4;
const METRIC_NETWORK_LATENCY: u32 = 5;
const METRIC_TASK_EXEC_TIME: u32 = 6;

// Alert IDs
const ALERT_HIGH_CPU: u32 = 1;
const ALERT_HIGH_MEMORY: u32 = 2;
const ALERT_HIGH_TEMP: u32 = 3;
const ALERT_LOW_BATTERY: u32 = 4;

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example 1: Basic Monitoring Setup
    basic_monitoring_setup();

    // Example 2: Metrics Collection
    metrics_collection_example();

    // Example 3: Alert Configuration
    alert_configuration_example();

    // Example 4: Health Monitoring
    health_monitoring_example();

    // Example 5: Time-Series Analysis
    timeseries_analysis_example();

    loop {}
}

fn basic_monitoring_setup() {
    let mut monitor = RemoteMonitor::new(system_time_ms);

    // Register system metrics
    let cpu_metric = MetricMetadata {
        id: METRIC_CPU_USAGE,
        metric_type: MetricType::Gauge,
        unit: 0,        // Percentage
        description: 0, // CPU Usage
    };
    monitor
        .register_metric(cpu_metric)
        .expect("Failed to register CPU metric");

    let memory_metric = MetricMetadata {
        id: METRIC_MEMORY_USAGE,
        metric_type: MetricType::Gauge,
        unit: 0,        // Percentage
        description: 1, // Memory Usage
    };
    monitor
        .register_metric(memory_metric)
        .expect("Failed to register memory metric");

    let temp_metric = MetricMetadata {
        id: METRIC_TEMPERATURE,
        metric_type: MetricType::Gauge,
        unit: 1,        // Celsius
        description: 2, // Temperature
    };
    monitor
        .register_metric(temp_metric)
        .expect("Failed to register temperature metric");

    // Configure alerts
    let high_cpu_alert = AlertDefinition {
        id: ALERT_HIGH_CPU,
        metric_id: METRIC_CPU_USAGE,
        condition: AlertCondition::Above,
        threshold: 80,
        severity: AlertSeverity::Warning,
        enabled: true,
    };
    monitor
        .define_alert(high_cpu_alert)
        .expect("Failed to define alert");

    // Record initial metrics
    monitor
        .record_metric(METRIC_CPU_USAGE, MetricValue::Percentage(25))
        .expect("Failed to record metric");
    monitor
        .record_metric(METRIC_MEMORY_USAGE, MetricValue::Percentage(45))
        .expect("Failed to record metric");
    monitor
        .record_metric(METRIC_TEMPERATURE, MetricValue::Integer(42))
        .expect("Failed to record metric");
}

fn metrics_collection_example() {
    let mut monitor = RemoteMonitor::new(system_time_ms);

    // Register metrics
    for &(id, metric_type) in &[
        (METRIC_CPU_USAGE, MetricType::Gauge),
        (METRIC_MEMORY_USAGE, MetricType::Gauge),
        (METRIC_TEMPERATURE, MetricType::Gauge),
        (METRIC_BATTERY_VOLTAGE, MetricType::Gauge),
        (METRIC_NETWORK_LATENCY, MetricType::Histogram),
        (METRIC_TASK_EXEC_TIME, MetricType::Histogram),
    ] {
        let metadata = MetricMetadata {
            id,
            metric_type,
            unit: 0,
            description: 0,
        };
        monitor
            .register_metric(metadata)
            .expect("Failed to register metric");
    }

    // Collect metrics over time
    for cycle in 0..100 {
        advance_time(100);

        // CPU usage (simulated)
        let cpu_usage = 20 + (cycle % 40);
        monitor
            .record_metric(METRIC_CPU_USAGE, MetricValue::Percentage(cpu_usage as u8))
            .expect("Failed");

        // Memory usage (gradually increasing)
        let memory_usage = 30 + (cycle / 5);
        monitor
            .record_metric(
                METRIC_MEMORY_USAGE,
                MetricValue::Percentage(memory_usage.min(95) as u8),
            )
            .expect("Failed");

        // Temperature (varies with CPU)
        let temperature = 40 + (cpu_usage / 2);
        monitor
            .record_metric(METRIC_TEMPERATURE, MetricValue::Integer(temperature as i64))
            .expect("Failed");

        // Battery voltage (slowly decreasing)
        let voltage = 4200 - (cycle * 2); // mV
        monitor
            .record_metric(METRIC_BATTERY_VOLTAGE, MetricValue::Integer(voltage as i64))
            .expect("Failed");

        // Network latency (variable)
        let latency = 10 + (cycle % 15);
        monitor
            .record_metric(METRIC_NETWORK_LATENCY, MetricValue::Integer(latency as i64))
            .expect("Failed");

        // Task execution time
        let exec_time = 50 + (cycle % 10);
        monitor
            .record_metric(
                METRIC_TASK_EXEC_TIME,
                MetricValue::Integer(exec_time as i64),
            )
            .expect("Failed");
    }

    // Get update counts
    let metrics = monitor.metrics();
    for metadata in metrics.metrics() {
        if let Some(_count) = metrics.update_count(metadata.id) {
            // Number of times this metric was recorded
        }
    }
}

fn alert_configuration_example() {
    let mut monitor = RemoteMonitor::new(system_time_ms);

    // Register metrics
    let cpu_metric = MetricMetadata {
        id: METRIC_CPU_USAGE,
        metric_type: MetricType::Gauge,
        unit: 0,
        description: 0,
    };
    monitor.register_metric(cpu_metric).expect("Failed");

    let memory_metric = MetricMetadata {
        id: METRIC_MEMORY_USAGE,
        metric_type: MetricType::Gauge,
        unit: 0,
        description: 1,
    };
    monitor.register_metric(memory_metric).expect("Failed");

    let temp_metric = MetricMetadata {
        id: METRIC_TEMPERATURE,
        metric_type: MetricType::Gauge,
        unit: 1,
        description: 2,
    };
    monitor.register_metric(temp_metric).expect("Failed");

    // Configure comprehensive alerts

    // CPU usage alerts
    let cpu_warning = AlertDefinition {
        id: ALERT_HIGH_CPU,
        metric_id: METRIC_CPU_USAGE,
        condition: AlertCondition::Above,
        threshold: 80,
        severity: AlertSeverity::Warning,
        enabled: true,
    };
    monitor.define_alert(cpu_warning).expect("Failed");

    let cpu_critical = AlertDefinition {
        id: ALERT_HIGH_CPU + 1,
        metric_id: METRIC_CPU_USAGE,
        condition: AlertCondition::Above,
        threshold: 95,
        severity: AlertSeverity::Critical,
        enabled: true,
    };
    monitor.define_alert(cpu_critical).expect("Failed");

    // Memory usage alerts
    let memory_warning = AlertDefinition {
        id: ALERT_HIGH_MEMORY,
        metric_id: METRIC_MEMORY_USAGE,
        condition: AlertCondition::Above,
        threshold: 85,
        severity: AlertSeverity::Warning,
        enabled: true,
    };
    monitor.define_alert(memory_warning).expect("Failed");

    // Temperature alerts
    let temp_critical = AlertDefinition {
        id: ALERT_HIGH_TEMP,
        metric_id: METRIC_TEMPERATURE,
        condition: AlertCondition::Above,
        threshold: 75,
        severity: AlertSeverity::Critical,
        enabled: true,
    };
    monitor.define_alert(temp_critical).expect("Failed");

    // Battery alerts
    let battery_low = AlertDefinition {
        id: ALERT_LOW_BATTERY,
        metric_id: METRIC_BATTERY_VOLTAGE,
        condition: AlertCondition::Below,
        threshold: 3500,
        severity: AlertSeverity::Warning,
        enabled: true,
    };
    monitor.define_alert(battery_low).expect("Failed");

    // Simulate conditions that trigger alerts
    monitor
        .record_metric(METRIC_CPU_USAGE, MetricValue::Percentage(85))
        .expect("Failed");
    monitor
        .record_metric(METRIC_MEMORY_USAGE, MetricValue::Percentage(90))
        .expect("Failed");
    monitor
        .record_metric(METRIC_TEMPERATURE, MetricValue::Integer(80))
        .expect("Failed");

    // Check for triggered alerts
    let alerts = monitor.alerts();
    let recent_events = alerts.recent_events(10);

    for event in recent_events.iter() {
        match event.severity {
            AlertSeverity::Critical => {
                // Critical alert - take immediate action
                handle_critical_alert(event.alert_id);
            }
            AlertSeverity::Warning => {
                // Warning alert - log and monitor
                log_warning_alert(event.alert_id);
            }
            _ => {}
        }
    }
}

fn handle_critical_alert(alert_id: u32) {
    if alert_id == ALERT_HIGH_TEMP {
        // Temperature too high - reduce clock speed or shutdown
    }
}

fn log_warning_alert(_alert_id: u32) {
    // Log warning to persistent storage
}

fn health_monitoring_example() {
    let monitor = RemoteMonitor::new(system_time_ms);

    // Collect system health data
    let cpu_usage = measure_cpu_usage();
    let memory_usage = measure_memory_usage();
    let temperature = measure_temperature();
    let active_faults = count_active_faults();

    // Generate health report
    let health = monitor.health_report(cpu_usage, memory_usage, temperature, active_faults);

    // Check health status
    match health.status {
        HealthStatus::Healthy => {
            // System operating normally
        }
        HealthStatus::Degraded => {
            // System experiencing minor issues
            // Continue operation with monitoring
        }
        HealthStatus::Unhealthy => {
            // System has significant issues
            // May need intervention
            check_and_recover();
        }
        HealthStatus::Critical => {
            // System in critical state
            // Immediate action required
            enter_safe_mode();
        }
    }

    // Calculate uptime
    let uptime_seconds = monitor.uptime_seconds();
    let _uptime_hours = uptime_seconds / 3600;
    let _uptime_days = _uptime_hours / 24;
}

fn measure_cpu_usage() -> u8 {
    // Measure actual CPU usage
    50
}

fn measure_memory_usage() -> u8 {
    // Measure actual memory usage
    60
}

fn measure_temperature() -> i16 {
    // Read temperature sensor
    45
}

fn count_active_faults() -> u16 {
    // Count active system faults
    0
}

fn check_and_recover() {
    // Attempt recovery procedures
}

fn enter_safe_mode() {
    // Enter safe mode for protection
}

fn timeseries_analysis_example() {
    let mut monitor = RemoteMonitor::new(system_time_ms);

    // Register metric
    let metric = MetricMetadata {
        id: METRIC_CPU_USAGE,
        metric_type: MetricType::Gauge,
        unit: 0,
        description: 0,
    };
    monitor.register_metric(metric).expect("Failed");

    // Collect time-series data
    for i in 0..128 {
        advance_time(1000);
        let value = 20 + (i % 60);
        monitor
            .record_metric(METRIC_CPU_USAGE, MetricValue::Percentage(value as u8))
            .expect("Failed");
    }

    // Analyze time-series data
    let metrics = monitor.metrics();
    if let Some(timeseries) = metrics.get_timeseries(METRIC_CPU_USAGE) {
        // Get recent data points
        let _recent = timeseries.recent(60); // Last 60 samples

        // Calculate average over last minute
        if let Some(avg) = timeseries.average(60) {
            // Average CPU usage over last 60 seconds
            if avg > 80 {
                // High average CPU usage
            }
        }

        // Analyze trends
        let total_points = timeseries.count();
        if total_points >= 128 {
            // Buffer is full, oldest data is being overwritten
        }
    }
}

// Example: Integration with communication protocol for remote access
fn _remote_telemetry_example() {
    let mut monitor = RemoteMonitor::new(system_time_ms);

    // Setup metrics and alerts
    _setup_monitoring(&mut monitor);

    // Periodically send telemetry data
    loop {
        advance_time(10000); // Every 10 seconds

        // Collect current metrics
        let cpu = measure_cpu_usage();
        let memory = measure_memory_usage();
        let temp = measure_temperature();
        let faults = count_active_faults();

        // Generate health report
        let health = monitor.health_report(cpu, memory, temp, faults);

        // Send telemetry via CoAP/MQTT-SN
        _send_telemetry(&health);

        // Check for critical alerts
        let alerts = monitor.alerts();
        let recent = alerts.recent_events(5);

        if !recent.is_empty() {
            // Send alert notifications
            _send_alerts(recent);
        }

        if cpu > 90 || memory > 90 {
            break;
        }
    }
}

fn _setup_monitoring(_monitor: &mut RemoteMonitor) {
    // Setup all metrics and alerts
}

fn _send_telemetry(_health: &mielin_rt::monitoring::HealthReport) {
    // Send via network
}

fn _send_alerts(_alerts: &[mielin_rt::monitoring::AlertEvent]) {
    // Send alert notifications
}
