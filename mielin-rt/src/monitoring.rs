//! Remote Monitoring Infrastructure
//!
//! This module provides comprehensive remote monitoring capabilities for
//! embedded systems, enabling real-time visibility into device health,
//! performance, and operational status.
//!
//! # Features
//!
//! - System health monitoring
//! - Performance metrics collection
//! - Resource utilization tracking
//! - Event streaming
//! - Alert generation
//! - Telemetry aggregation
//! - Remote diagnostics
//! - Time-series data management
//!
//! # Metrics
//!
//! - CPU usage
//! - Memory utilization
//! - Power consumption
//! - Temperature
//! - Network statistics
//! - Task execution times
//! - Fault counts
//! - Uptime tracking

#![allow(dead_code)]

use core::fmt;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of metrics
pub const MAX_METRICS: usize = 64;

/// Maximum number of alerts
pub const MAX_ALERTS: usize = 32;

/// Maximum number of subscribers
pub const MAX_SUBSCRIBERS: usize = 8;

/// Maximum time-series data points
pub const MAX_TIMESERIES_POINTS: usize = 128;

/// Monitoring error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitoringError {
    /// Metric not found
    MetricNotFound,
    /// Too many metrics
    TooManyMetrics,
    /// Too many alerts
    TooManyAlerts,
    /// Too many subscribers
    TooManySubscribers,
    /// Invalid metric value
    InvalidValue,
    /// Not initialized
    NotInitialized,
}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetricNotFound => write!(f, "Metric not found"),
            Self::TooManyMetrics => write!(f, "Too many metrics"),
            Self::TooManyAlerts => write!(f, "Too many alerts"),
            Self::TooManySubscribers => write!(f, "Too many subscribers"),
            Self::InvalidValue => write!(f, "Invalid metric value"),
            Self::NotInitialized => write!(f, "Not initialized"),
        }
    }
}

/// Metric type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    /// Counter (monotonically increasing)
    Counter,
    /// Gauge (can go up or down)
    Gauge,
    /// Histogram (statistical distribution)
    Histogram,
}

/// Metric value
#[derive(Debug, Clone, Copy)]
pub enum MetricValue {
    /// Integer value
    Integer(i64),
    /// Floating point value (stored as fixed-point for no_std)
    FixedPoint(i64), // Value * 1000 for 3 decimal places
    /// Percentage (0-100)
    Percentage(u8),
}

impl MetricValue {
    /// Create a fixed-point value from a float-like representation
    pub const fn from_millis(millis: i64) -> Self {
        Self::FixedPoint(millis)
    }

    /// Get integer value
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Get percentage value
    pub const fn as_percentage(&self) -> Option<u8> {
        match self {
            Self::Percentage(v) => Some(*v),
            _ => None,
        }
    }
}

/// Metric metadata
#[derive(Debug, Clone, Copy)]
pub struct MetricMetadata {
    /// Metric ID
    pub id: u32,
    /// Metric type
    pub metric_type: MetricType,
    /// Unit (encoded as u32 for no_std)
    pub unit: u32,
    /// Description (encoded as u32 for no_std)
    pub description: u32,
}

/// Metric data point
#[derive(Debug, Clone, Copy)]
pub struct MetricDataPoint {
    /// Metric ID
    pub metric_id: u32,
    /// Timestamp (milliseconds)
    pub timestamp_ms: u32,
    /// Value
    pub value: MetricValue,
}

/// Alert severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    /// Informational
    Info,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Critical
    Critical,
}

/// Alert condition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertCondition {
    /// Value above threshold
    Above,
    /// Value below threshold
    Below,
    /// Value changed
    Changed,
    /// Rate of change exceeded
    RateExceeded,
}

/// Alert definition
#[derive(Debug, Clone, Copy)]
pub struct AlertDefinition {
    /// Alert ID
    pub id: u32,
    /// Metric ID to monitor
    pub metric_id: u32,
    /// Alert condition
    pub condition: AlertCondition,
    /// Threshold value
    pub threshold: i64,
    /// Severity
    pub severity: AlertSeverity,
    /// Enabled
    pub enabled: bool,
}

/// Alert event
#[derive(Debug, Clone, Copy)]
pub struct AlertEvent {
    /// Alert ID
    pub alert_id: u32,
    /// Timestamp
    pub timestamp_ms: u32,
    /// Current value
    pub current_value: MetricValue,
    /// Threshold value
    pub threshold: i64,
    /// Severity
    pub severity: AlertSeverity,
}

/// System health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Healthy
    Healthy,
    /// Degraded performance
    Degraded,
    /// Unhealthy
    Unhealthy,
    /// Critical
    Critical,
}

/// System health report
#[derive(Debug, Clone, Copy)]
pub struct HealthReport {
    /// Overall health status
    pub status: HealthStatus,
    /// CPU usage percentage
    pub cpu_usage: u8,
    /// Memory usage percentage
    pub memory_usage: u8,
    /// Temperature (degrees Celsius)
    pub temperature: i16,
    /// Uptime (seconds)
    pub uptime_seconds: u32,
    /// Active faults
    pub active_faults: u16,
    /// Timestamp
    pub timestamp_ms: u32,
}

impl HealthReport {
    /// Create a new health report
    pub const fn new(timestamp_ms: u32) -> Self {
        Self {
            status: HealthStatus::Healthy,
            cpu_usage: 0,
            memory_usage: 0,
            temperature: 0,
            uptime_seconds: 0,
            active_faults: 0,
            timestamp_ms,
        }
    }

    /// Calculate overall health status
    pub fn calculate_status(&mut self) {
        if self.active_faults > 10
            || self.cpu_usage > 95
            || self.memory_usage > 95
            || self.temperature > 85
        {
            self.status = HealthStatus::Critical;
        } else if self.active_faults > 5
            || self.cpu_usage > 80
            || self.memory_usage > 80
            || self.temperature > 75
        {
            self.status = HealthStatus::Unhealthy;
        } else if self.active_faults > 0 || self.cpu_usage > 60 || self.memory_usage > 70 {
            self.status = HealthStatus::Degraded;
        } else {
            self.status = HealthStatus::Healthy;
        }
    }
}

/// Time-series data buffer
pub struct TimeSeriesBuffer {
    /// Data points
    points: [Option<MetricDataPoint>; MAX_TIMESERIES_POINTS],
    /// Write index
    write_index: usize,
    /// Point count
    count: usize,
}

impl Default for TimeSeriesBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSeriesBuffer {
    /// Create a new time-series buffer
    pub const fn new() -> Self {
        Self {
            points: [None; MAX_TIMESERIES_POINTS],
            write_index: 0,
            count: 0,
        }
    }

    /// Add a data point
    pub fn add(&mut self, point: MetricDataPoint) {
        self.points[self.write_index] = Some(point);
        self.write_index = (self.write_index + 1) % MAX_TIMESERIES_POINTS;

        if self.count < MAX_TIMESERIES_POINTS {
            self.count += 1;
        }
    }

    /// Get recent points
    pub fn recent(&self, count: usize) -> heapless::Vec<MetricDataPoint, MAX_TIMESERIES_POINTS> {
        let mut result = heapless::Vec::new();
        let take = count.min(self.count);

        let mut idx = if self.write_index == 0 {
            MAX_TIMESERIES_POINTS - 1
        } else {
            self.write_index - 1
        };

        for _ in 0..take {
            if let Some(point) = self.points[idx] {
                let _ = result.push(point);
            }

            idx = if idx == 0 {
                MAX_TIMESERIES_POINTS - 1
            } else {
                idx - 1
            };
        }

        result
    }

    /// Calculate average over recent points
    pub fn average(&self, count: usize) -> Option<i64> {
        let points = self.recent(count);
        if points.is_empty() {
            return None;
        }

        let sum: i64 = points
            .iter()
            .map(|p| match p.value {
                MetricValue::Integer(v) => v,
                MetricValue::FixedPoint(v) => v,
                MetricValue::Percentage(v) => v as i64,
            })
            .sum();

        Some(sum / points.len() as i64)
    }

    /// Get point count
    pub const fn count(&self) -> usize {
        self.count
    }
}

/// Metric registry
pub struct MetricRegistry {
    /// Metrics
    metrics: heapless::Vec<MetricMetadata, MAX_METRICS>,
    /// Time-series data
    timeseries: [TimeSeriesBuffer; MAX_METRICS],
    /// Metric update counters
    update_counts: [AtomicU32; MAX_METRICS],
}

impl Default for MetricRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricRegistry {
    /// Create a new metric registry
    pub const fn new() -> Self {
        const EMPTY_BUFFER: TimeSeriesBuffer = TimeSeriesBuffer::new();
        // Note: Using const here is acceptable for initialization in const context
        // despite clippy warning about interior mutability
        #[allow(clippy::declare_interior_mutable_const)]
        const EMPTY_COUNTER: AtomicU32 = AtomicU32::new(0);

        Self {
            metrics: heapless::Vec::new(),
            timeseries: [EMPTY_BUFFER; MAX_METRICS],
            update_counts: [EMPTY_COUNTER; MAX_METRICS],
        }
    }

    /// Register a metric
    pub fn register(&mut self, metadata: MetricMetadata) -> Result<(), MonitoringError> {
        if self.metrics.len() >= MAX_METRICS {
            return Err(MonitoringError::TooManyMetrics);
        }

        self.metrics
            .push(metadata)
            .map_err(|_| MonitoringError::TooManyMetrics)?;
        Ok(())
    }

    /// Record a metric value
    pub fn record(
        &mut self,
        metric_id: u32,
        value: MetricValue,
        timestamp_ms: u32,
    ) -> Result<(), MonitoringError> {
        let index = self
            .metrics
            .iter()
            .position(|m| m.id == metric_id)
            .ok_or(MonitoringError::MetricNotFound)?;

        let point = MetricDataPoint {
            metric_id,
            timestamp_ms,
            value,
        };

        self.timeseries[index].add(point);
        self.update_counts[index].fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Get metric time-series
    pub fn get_timeseries(&self, metric_id: u32) -> Option<&TimeSeriesBuffer> {
        let index = self.metrics.iter().position(|m| m.id == metric_id)?;
        Some(&self.timeseries[index])
    }

    /// Get update count for a metric
    pub fn update_count(&self, metric_id: u32) -> Option<u32> {
        let index = self.metrics.iter().position(|m| m.id == metric_id)?;
        Some(self.update_counts[index].load(Ordering::Relaxed))
    }

    /// Get all metrics
    pub fn metrics(&self) -> &[MetricMetadata] {
        &self.metrics
    }
}

/// Alert manager
pub struct AlertManager {
    /// Alert definitions
    alerts: heapless::Vec<AlertDefinition, MAX_ALERTS>,
    /// Recent alert events
    events: heapless::Vec<AlertEvent, MAX_ALERTS>,
    /// Alert trigger counters
    trigger_counts: [AtomicU32; MAX_ALERTS],
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertManager {
    /// Create a new alert manager
    pub const fn new() -> Self {
        // Note: Using const here is acceptable for initialization in const context
        // despite clippy warning about interior mutability
        #[allow(clippy::declare_interior_mutable_const)]
        const EMPTY_COUNTER: AtomicU32 = AtomicU32::new(0);

        Self {
            alerts: heapless::Vec::new(),
            events: heapless::Vec::new(),
            trigger_counts: [EMPTY_COUNTER; MAX_ALERTS],
        }
    }

    /// Define an alert
    pub fn define_alert(&mut self, alert: AlertDefinition) -> Result<(), MonitoringError> {
        if self.alerts.len() >= MAX_ALERTS {
            return Err(MonitoringError::TooManyAlerts);
        }

        self.alerts
            .push(alert)
            .map_err(|_| MonitoringError::TooManyAlerts)?;
        Ok(())
    }

    /// Check metrics against alert conditions
    pub fn check_alerts(
        &mut self,
        metric_id: u32,
        current_value: MetricValue,
        timestamp_ms: u32,
    ) -> heapless::Vec<AlertEvent, MAX_ALERTS> {
        let mut triggered = heapless::Vec::new();

        for (index, alert) in self.alerts.iter().enumerate() {
            if !alert.enabled || alert.metric_id != metric_id {
                continue;
            }

            let value_i64 = match current_value {
                MetricValue::Integer(v) => v,
                MetricValue::FixedPoint(v) => v,
                MetricValue::Percentage(v) => v as i64,
            };

            let should_trigger = match alert.condition {
                AlertCondition::Above => value_i64 > alert.threshold,
                AlertCondition::Below => value_i64 < alert.threshold,
                AlertCondition::Changed => true, // Would need previous value
                AlertCondition::RateExceeded => false, // Would need time-series
            };

            if should_trigger {
                let event = AlertEvent {
                    alert_id: alert.id,
                    timestamp_ms,
                    current_value,
                    threshold: alert.threshold,
                    severity: alert.severity,
                };

                let _ = triggered.push(event);
                let _ = self.events.push(event);
                self.trigger_counts[index].fetch_add(1, Ordering::Relaxed);
            }
        }

        triggered
    }

    /// Get trigger count for an alert
    pub fn trigger_count(&self, alert_id: u32) -> Option<u32> {
        let index = self.alerts.iter().position(|a| a.id == alert_id)?;
        Some(self.trigger_counts[index].load(Ordering::Relaxed))
    }

    /// Get recent alert events
    pub fn recent_events(&self, count: usize) -> &[AlertEvent] {
        let len = self.events.len().min(count);
        &self.events[self.events.len().saturating_sub(len)..]
    }
}

/// Remote monitoring manager
pub struct RemoteMonitor {
    /// Metric registry
    metrics: MetricRegistry,
    /// Alert manager
    alerts: AlertManager,
    /// System start time
    start_time_ms: u32,
    /// Last health check time
    last_health_check_ms: AtomicU32,
    /// System time getter
    system_time_ms: fn() -> u32,
    /// Initialized
    initialized: bool,
}

impl RemoteMonitor {
    /// Create a new remote monitor
    pub fn new(system_time_ms: fn() -> u32) -> Self {
        let start_time = system_time_ms();

        Self {
            metrics: MetricRegistry::new(),
            alerts: AlertManager::new(),
            start_time_ms: start_time,
            last_health_check_ms: AtomicU32::new(start_time),
            system_time_ms,
            initialized: true,
        }
    }

    /// Register a metric
    pub fn register_metric(&mut self, metadata: MetricMetadata) -> Result<(), MonitoringError> {
        self.metrics.register(metadata)
    }

    /// Record a metric value
    pub fn record_metric(
        &mut self,
        metric_id: u32,
        value: MetricValue,
    ) -> Result<(), MonitoringError> {
        if !self.initialized {
            return Err(MonitoringError::NotInitialized);
        }

        let timestamp = (self.system_time_ms)();
        self.metrics.record(metric_id, value, timestamp)?;

        // Check alerts
        let _ = self.alerts.check_alerts(metric_id, value, timestamp);

        Ok(())
    }

    /// Define an alert
    pub fn define_alert(&mut self, alert: AlertDefinition) -> Result<(), MonitoringError> {
        self.alerts.define_alert(alert)
    }

    /// Get health report
    pub fn health_report(
        &self,
        cpu_usage: u8,
        memory_usage: u8,
        temperature: i16,
        active_faults: u16,
    ) -> HealthReport {
        let current_time = (self.system_time_ms)();
        let uptime_seconds = (current_time - self.start_time_ms) / 1000;

        let mut report = HealthReport {
            status: HealthStatus::Healthy,
            cpu_usage,
            memory_usage,
            temperature,
            uptime_seconds,
            active_faults,
            timestamp_ms: current_time,
        };

        report.calculate_status();
        self.last_health_check_ms
            .store(current_time, Ordering::Release);

        report
    }

    /// Get metric registry
    pub const fn metrics(&self) -> &MetricRegistry {
        &self.metrics
    }

    /// Get alert manager
    pub const fn alerts(&self) -> &AlertManager {
        &self.alerts
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u32 {
        let current_time = (self.system_time_ms)();
        (current_time - self.start_time_ms) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_metric_value() {
        let int_val = MetricValue::Integer(42);
        assert_eq!(int_val.as_integer(), Some(42));

        let pct_val = MetricValue::Percentage(75);
        assert_eq!(pct_val.as_percentage(), Some(75));

        let fixed_val = MetricValue::from_millis(1234);
        assert!(matches!(fixed_val, MetricValue::FixedPoint(1234)));
    }

    #[test]
    fn test_health_report() {
        let mut report = HealthReport::new(1000);
        report.cpu_usage = 50;
        report.memory_usage = 60;
        report.temperature = 45;
        report.active_faults = 0;

        report.calculate_status();
        assert_eq!(report.status, HealthStatus::Healthy);

        report.cpu_usage = 85;
        report.calculate_status();
        assert_eq!(report.status, HealthStatus::Unhealthy);

        report.cpu_usage = 98;
        report.calculate_status();
        assert_eq!(report.status, HealthStatus::Critical);
    }

    #[test]
    fn test_timeseries_buffer() {
        let mut buffer = TimeSeriesBuffer::new();

        let point1 = MetricDataPoint {
            metric_id: 1,
            timestamp_ms: 1000,
            value: MetricValue::Integer(10),
        };

        let point2 = MetricDataPoint {
            metric_id: 1,
            timestamp_ms: 2000,
            value: MetricValue::Integer(20),
        };

        buffer.add(point1);
        buffer.add(point2);

        assert_eq!(buffer.count(), 2);

        let recent = buffer.recent(10);
        assert_eq!(recent.len(), 2);

        let avg = buffer.average(2);
        assert_eq!(avg, Some(15));
    }

    #[test]
    fn test_metric_registry() {
        let mut registry = MetricRegistry::new();

        let metadata = MetricMetadata {
            id: 1,
            metric_type: MetricType::Gauge,
            unit: 0,
            description: 0,
        };

        assert!(registry.register(metadata).is_ok());

        assert!(registry.record(1, MetricValue::Integer(42), 1000).is_ok());
        assert_eq!(registry.update_count(1), Some(1));

        let timeseries = registry.get_timeseries(1);
        assert!(timeseries.is_some());
        assert_eq!(timeseries.unwrap().count(), 1);
    }

    #[test]
    fn test_alert_manager() {
        let mut alert_mgr = AlertManager::new();

        let alert = AlertDefinition {
            id: 1,
            metric_id: 1,
            condition: AlertCondition::Above,
            threshold: 50,
            severity: AlertSeverity::Warning,
            enabled: true,
        };

        assert!(alert_mgr.define_alert(alert).is_ok());

        // Should not trigger
        let events = alert_mgr.check_alerts(1, MetricValue::Integer(30), 1000);
        assert_eq!(events.len(), 0);

        // Should trigger
        let events = alert_mgr.check_alerts(1, MetricValue::Integer(60), 2000);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].alert_id, 1);
        assert_eq!(events[0].severity, AlertSeverity::Warning);
    }

    #[test]
    fn test_remote_monitor() {
        fn mock_time() -> u32 {
            1000
        }

        let mut monitor = RemoteMonitor::new(mock_time);

        let metadata = MetricMetadata {
            id: 1,
            metric_type: MetricType::Counter,
            unit: 0,
            description: 0,
        };

        assert!(monitor.register_metric(metadata).is_ok());
        assert!(monitor.record_metric(1, MetricValue::Integer(100)).is_ok());

        let health = monitor.health_report(50, 60, 45, 0);
        assert_eq!(health.cpu_usage, 50);
        assert_eq!(health.memory_usage, 60);
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_monitoring_error_display() {
        assert_eq!(
            format!("{}", MonitoringError::MetricNotFound),
            "Metric not found"
        );
        assert_eq!(
            format!("{}", MonitoringError::TooManyMetrics),
            "Too many metrics"
        );
    }
}
