//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    use crate::resource::types::*;
    #[test]
    fn test_memory_quota_default() {
        let quota = MemoryQuota::default();
        assert_eq!(quota.max_heap_bytes, 256 * 1024 * 1024);
        assert_eq!(quota.soft_limit_percent, 80);
    }
    #[test]
    fn test_memory_quota_unlimited() {
        let quota = MemoryQuota::unlimited();
        assert_eq!(quota.max_heap_bytes, u64::MAX);
    }
    #[test]
    fn test_memory_quota_soft_limit() {
        let quota = MemoryQuota::with_limits(100, 10, 110);
        assert_eq!(quota.heap_soft_limit(), 80);
        assert_eq!(quota.total_soft_limit(), 88);
    }
    #[test]
    fn test_cpu_quota_default() {
        let quota = CpuQuota::default();
        assert_eq!(quota.max_execution_time_us, 10_000_000);
        assert_eq!(quota.priority, 50);
    }
    #[test]
    fn test_cpu_quota_utilization() {
        let quota = CpuQuota::with_limits(1_000_000, 500_000);
        assert_eq!(quota.utilization_percent(), 50);
    }
    #[test]
    fn test_cpu_quota_priority() {
        let quota = CpuQuota::default().with_priority(80);
        assert_eq!(quota.priority, 80);
        let quota = CpuQuota::default().with_priority(150);
        assert_eq!(quota.priority, 100);
    }
    #[test]
    fn test_network_quota_default() {
        let quota = NetworkQuota::default();
        assert_eq!(quota.max_send_bytes_per_sec, 10 * 1024 * 1024);
        assert_eq!(quota.max_connections, 100);
    }
    #[test]
    fn test_network_quota_bandwidth() {
        let quota = NetworkQuota::with_bandwidth(1000, 2000);
        assert_eq!(quota.max_send_bytes_per_sec, 1000);
        assert_eq!(quota.max_recv_bytes_per_sec, 2000);
        assert_eq!(quota.max_total_bytes_per_sec, 3000);
    }
    #[test]
    fn test_storage_quota_default() {
        let quota = StorageQuota::default();
        assert_eq!(quota.max_persistent_bytes, 1024 * 1024 * 1024);
        assert_eq!(quota.max_files, 10000);
    }
    #[test]
    fn test_resource_quota_default() {
        let quota = ResourceQuota::default();
        assert!(quota.enforced);
        assert_eq!(quota.memory.max_heap_bytes, 256 * 1024 * 1024);
    }
    #[test]
    fn test_resource_quota_unlimited() {
        let quota = ResourceQuota::unlimited();
        assert!(!quota.enforced);
        assert_eq!(quota.memory.max_heap_bytes, u64::MAX);
    }
    #[test]
    fn test_resource_quota_minimal() {
        let quota = ResourceQuota::minimal();
        assert!(quota.enforced);
        assert_eq!(quota.memory.max_heap_bytes, 16 * 1024 * 1024);
    }
    #[test]
    fn test_resource_quota_high_performance() {
        let quota = ResourceQuota::high_performance();
        assert!(quota.enforced);
        assert_eq!(quota.cpu.priority, 80);
    }
    #[test]
    fn test_resource_usage_network_total() {
        let usage = ResourceUsage {
            bytes_sent_this_second: 100,
            bytes_received_this_second: 200,
            ..Default::default()
        };
        assert_eq!(usage.network_bytes_this_second(), 300);
    }
    #[test]
    fn test_resource_usage_storage_total() {
        let usage = ResourceUsage {
            persistent_storage_bytes: 1000,
            temp_storage_bytes: 500,
            ..Default::default()
        };
        assert_eq!(usage.total_storage_bytes(), 1500);
    }
    #[test]
    fn test_quota_violation_is_warning() {
        assert!(QuotaViolation::MemorySoftLimitReached.is_warning());
        assert!(!QuotaViolation::HeapMemoryExceeded.is_warning());
    }
    #[test]
    fn test_quota_violation_description() {
        let desc = QuotaViolation::HeapMemoryExceeded.description();
        assert!(!desc.is_empty());
    }
    #[test]
    fn test_quota_check_result_ok() {
        let result = QuotaCheckResult::ok();
        assert!(result.within_limits);
        assert!(result.violations.is_empty());
        assert!(result.warnings.is_empty());
    }
    #[test]
    fn test_quota_check_result_with_violation() {
        let result = QuotaCheckResult::ok().with_violation(QuotaViolation::HeapMemoryExceeded);
        assert!(!result.within_limits);
        assert_eq!(result.violations.len(), 1);
    }
    #[test]
    fn test_quota_check_result_with_warning() {
        let result = QuotaCheckResult::ok().with_violation(QuotaViolation::MemorySoftLimitReached);
        assert!(result.within_limits);
        assert!(result.has_warnings());
        assert_eq!(result.warnings.len(), 1);
    }
    #[test]
    fn test_quota_enforcer_creation() {
        let enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert_eq!(enforcer.usage().heap_bytes, 0);
        assert_eq!(enforcer.violation_count(), 0);
    }
    #[test]
    fn test_quota_enforcer_memory_allocation() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_allocate_memory(1000));
        enforcer.record_allocation(1000);
        assert_eq!(enforcer.usage().heap_bytes, 1000);
        enforcer.record_deallocation(500);
        assert_eq!(enforcer.usage().heap_bytes, 500);
    }
    #[test]
    fn test_quota_enforcer_memory_limit() {
        let enforcer = QuotaEnforcer::new(ResourceQuota::minimal());
        assert!(enforcer.can_allocate_memory(1024 * 1024));
        assert!(!enforcer.can_allocate_memory(100 * 1024 * 1024));
    }
    #[test]
    fn test_quota_enforcer_cpu_time() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_use_cpu(1000));
        enforcer.record_cpu_time(1000);
        assert_eq!(enforcer.usage().cpu_time_this_second_us, 1000);
        assert_eq!(enforcer.usage().total_cpu_time_us, 1000);
    }
    #[test]
    fn test_quota_enforcer_network() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_send(1000));
        enforcer.record_send(1000);
        assert_eq!(enforcer.usage().bytes_sent_this_second, 1000);
        assert!(enforcer.can_receive(2000));
        enforcer.record_receive(2000);
        assert_eq!(enforcer.usage().bytes_received_this_second, 2000);
    }
    #[test]
    fn test_quota_enforcer_messages() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_send_message());
        enforcer.record_message();
        assert_eq!(enforcer.usage().messages_this_second, 1);
    }
    #[test]
    fn test_quota_enforcer_connections() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_open_connection());
        enforcer.record_connection_open();
        assert_eq!(enforcer.usage().active_connections, 1);
        enforcer.record_connection_close();
        assert_eq!(enforcer.usage().active_connections, 0);
    }
    #[test]
    fn test_quota_enforcer_storage() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_use_storage(1000, true));
        enforcer.record_storage(1000, true);
        assert_eq!(enforcer.usage().persistent_storage_bytes, 1000);
        assert!(enforcer.can_use_storage(500, false));
        enforcer.record_storage(500, false);
        assert_eq!(enforcer.usage().temp_storage_bytes, 500);
        enforcer.record_storage_freed(200, true);
        assert_eq!(enforcer.usage().persistent_storage_bytes, 800);
    }
    #[test]
    fn test_quota_enforcer_files() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        assert!(enforcer.can_create_file(1000));
        enforcer.record_file_created();
        assert_eq!(enforcer.usage().file_count, 1);
        enforcer.record_file_deleted();
        assert_eq!(enforcer.usage().file_count, 0);
    }
    #[test]
    fn test_quota_enforcer_check_within_limits() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        let result = enforcer.check();
        assert!(result.within_limits);
    }
    #[test]
    fn test_quota_enforcer_check_exceeds_limits() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::minimal());
        enforcer.record_allocation(100 * 1024 * 1024);
        let result = enforcer.check();
        assert!(!result.within_limits);
        assert!(result
            .violations
            .contains(&QuotaViolation::HeapMemoryExceeded));
    }
    #[test]
    fn test_quota_enforcer_reset_usage() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::default());
        enforcer.record_allocation(1000);
        enforcer.record_cpu_time(100);
        enforcer.reset_usage();
        assert_eq!(enforcer.usage().heap_bytes, 0);
        assert_eq!(enforcer.usage().total_cpu_time_us, 0);
    }
    #[test]
    fn test_quota_enforcer_unenforced() {
        let mut enforcer = QuotaEnforcer::new(ResourceQuota::unlimited());
        assert!(enforcer.can_allocate_memory(u64::MAX - 1));
        assert!(enforcer.can_use_cpu(u64::MAX - 1));
        assert!(enforcer.can_send(u64::MAX - 1));
    }
    #[test]
    fn test_atomic_resource_counter() {
        let counter = AtomicResourceCounter::new(100);
        assert_eq!(counter.get(), 100);
        counter.add(50);
        assert_eq!(counter.get(), 150);
        counter.sub(30);
        assert_eq!(counter.get(), 120);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }
    #[test]
    fn test_atomic_resource_counter_saturating_sub() {
        let counter = AtomicResourceCounter::new(10);
        counter.sub(20);
        assert_eq!(counter.get(), 0);
    }
    #[test]
    fn test_resource_manager_creation() {
        let manager = ResourceManager::new();
        assert_eq!(manager.agent_count(), 0);
        assert_eq!(manager.total_memory_used(), 0);
    }
    #[test]
    fn test_resource_manager_register_agent() {
        let mut manager = ResourceManager::new();
        let agent_id = [1u8; 16];
        manager.register_agent(agent_id);
        assert_eq!(manager.agent_count(), 1);
        assert!(manager.get_enforcer(&agent_id).is_some());
    }
    #[test]
    fn test_resource_manager_register_with_quota() {
        let mut manager = ResourceManager::new();
        let agent_id = [1u8; 16];
        manager.register_agent_with_quota(agent_id, ResourceQuota::minimal());
        let enforcer = manager.get_enforcer(&agent_id).unwrap();
        assert_eq!(enforcer.quota().memory.max_heap_bytes, 16 * 1024 * 1024);
    }
    #[test]
    fn test_resource_manager_unregister_agent() {
        let mut manager = ResourceManager::new();
        let agent_id = [1u8; 16];
        manager.register_agent(agent_id);
        assert_eq!(manager.agent_count(), 1);
        manager.unregister_agent(&agent_id);
        assert_eq!(manager.agent_count(), 0);
    }
    #[test]
    fn test_resource_manager_update_quota() {
        let mut manager = ResourceManager::new();
        let agent_id = [1u8; 16];
        manager.register_agent(agent_id);
        manager.update_quota(&agent_id, ResourceQuota::high_performance());
        let enforcer = manager.get_enforcer(&agent_id).unwrap();
        assert_eq!(enforcer.quota().cpu.priority, 80);
    }
    #[test]
    fn test_resource_manager_global_memory() {
        let mut manager = ResourceManager::new();
        manager.set_global_memory_limit(1000);
        assert!(manager.can_allocate_global(500));
        assert!(!manager.can_allocate_global(1500));
        manager.record_global_allocation(500);
        assert_eq!(manager.total_memory_used(), 500);
        manager.record_global_deallocation(200);
        assert_eq!(manager.total_memory_used(), 300);
    }
    #[test]
    fn test_resource_manager_usage_summary() {
        let mut manager = ResourceManager::new();
        let agent_id1 = [1u8; 16];
        let agent_id2 = [2u8; 16];
        manager.register_agent(agent_id1);
        manager.register_agent(agent_id2);
        if let Some(enforcer) = manager.get_enforcer_mut(&agent_id1) {
            enforcer.record_allocation(1000);
        }
        let summary = manager.usage_summary();
        assert_eq!(summary.len(), 2);
    }
    #[test]
    fn test_resource_manager_set_default_quota() {
        let mut manager = ResourceManager::new();
        manager.set_default_quota(ResourceQuota::minimal());
        let agent_id = [1u8; 16];
        manager.register_agent(agent_id);
        let enforcer = manager.get_enforcer(&agent_id).unwrap();
        assert_eq!(enforcer.quota().memory.max_heap_bytes, 16 * 1024 * 1024);
    }
    #[test]
    fn test_resource_snapshot_from_usage() {
        let usage = ResourceUsage {
            total_memory_bytes: 1000,
            total_cpu_time_us: 500,
            total_bytes_sent: 100,
            total_bytes_received: 200,
            persistent_storage_bytes: 300,
            temp_storage_bytes: 400,
            ..Default::default()
        };
        let snapshot = ResourceSnapshot::from_usage(&usage, 1_000_000);
        assert_eq!(snapshot.memory_bytes, 1000);
        assert_eq!(snapshot.cpu_time_us, 500);
        assert_eq!(snapshot.network_sent_bytes, 100);
        assert_eq!(snapshot.network_recv_bytes, 200);
        assert_eq!(snapshot.storage_bytes, 700);
        assert_eq!(snapshot.timestamp_us, 1_000_000);
    }
    #[test]
    fn test_resource_snapshot_rate_from() {
        let s1 = ResourceSnapshot {
            timestamp_us: 0,
            memory_bytes: 1000,
            cpu_time_us: 0,
            network_sent_bytes: 0,
            network_recv_bytes: 0,
            storage_bytes: 100,
        };
        let s2 = ResourceSnapshot {
            timestamp_us: 1_000_000,
            memory_bytes: 2000,
            cpu_time_us: 500_000,
            network_sent_bytes: 1000,
            network_recv_bytes: 2000,
            storage_bytes: 200,
        };
        let rate = s2.rate_from(&s1).unwrap();
        assert_eq!(rate.memory_bytes_per_sec, 1000.0);
        assert_eq!(rate.cpu_percent, 50.0);
        assert_eq!(rate.network_sent_bytes_per_sec, 1000.0);
        assert_eq!(rate.network_recv_bytes_per_sec, 2000.0);
        assert_eq!(rate.storage_bytes_per_sec, 100.0);
    }
    #[test]
    fn test_resource_rate_methods() {
        let rate = ResourceRate {
            memory_bytes_per_sec: 100.0,
            cpu_percent: 50.0,
            network_sent_bytes_per_sec: 1000.0,
            network_recv_bytes_per_sec: 2000.0,
            storage_bytes_per_sec: 50.0,
        };
        assert!(rate.is_memory_growing());
        assert!(rate.is_storage_growing());
        assert_eq!(rate.total_network_bytes_per_sec(), 3000.0);
    }
    #[test]
    fn test_history_config_presets() {
        let default = HistoryConfig::default();
        assert_eq!(default.max_snapshots, 1000);
        assert_eq!(default.min_interval_us, 1_000_000);
        let high = HistoryConfig::high_resolution();
        assert_eq!(high.max_snapshots, 10000);
        assert_eq!(high.min_interval_us, 100_000);
        let low = HistoryConfig::low_resolution();
        assert_eq!(low.max_snapshots, 100);
        assert_eq!(low.min_interval_us, 60_000_000);
    }
    #[test]
    fn test_resource_history_creation() {
        let agent_id = [1u8; 16];
        let history = ResourceHistory::new(agent_id, HistoryConfig::default());
        assert_eq!(history.agent_id(), &agent_id);
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }
    #[test]
    fn test_resource_history_record() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        let usage = ResourceUsage {
            total_memory_bytes: 1000,
            ..Default::default()
        };
        history.record(&usage, 1_000_000);
        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        let latest = history.latest().unwrap();
        assert_eq!(latest.memory_bytes, 1000);
    }
    #[test]
    fn test_resource_history_maybe_record() {
        let agent_id = [1u8; 16];
        let config = HistoryConfig {
            max_snapshots: 100,
            min_interval_us: 1_000_000,
        };
        let mut history = ResourceHistory::new(agent_id, config);
        let usage = ResourceUsage::default();
        assert!(history.maybe_record(&usage, 1_000_000));
        assert_eq!(history.len(), 1);
        assert!(!history.maybe_record(&usage, 1_500_000));
        assert_eq!(history.len(), 1);
        assert!(history.maybe_record(&usage, 2_000_000));
        assert_eq!(history.len(), 2);
    }
    #[test]
    fn test_resource_history_max_snapshots() {
        let agent_id = [1u8; 16];
        let config = HistoryConfig {
            max_snapshots: 3,
            min_interval_us: 0,
        };
        let mut history = ResourceHistory::new(agent_id, config);
        let usage = ResourceUsage::default();
        for i in 0..5 {
            history.record(&usage, i * 1000);
        }
        assert_eq!(history.len(), 3);
        assert_eq!(history.snapshots()[0].timestamp_us, 2000);
    }
    #[test]
    fn test_resource_history_average() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..3 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        let avg = history.average().unwrap();
        assert_eq!(avg.memory_bytes, 2000);
    }
    #[test]
    fn test_resource_history_peak() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in [100u64, 500, 300, 200] {
            let usage = ResourceUsage {
                total_memory_bytes: i,
                total_cpu_time_us: i * 2,
                ..Default::default()
            };
            history.record(&usage, i * 1000);
        }
        assert_eq!(history.peak_memory(), 500);
        assert_eq!(history.peak_cpu(), 1000);
    }
    #[test]
    fn test_resource_history_time_range() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..5 {
            let usage = ResourceUsage::default();
            history.record(&usage, i * 1_000_000);
        }
        let range = history.snapshots_in_range(1_000_000, 3_000_000);
        assert_eq!(range.len(), 3);
    }
    #[test]
    fn test_resource_history_clear() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        history.record(&ResourceUsage::default(), 0);
        assert!(!history.is_empty());
        history.clear();
        assert!(history.is_empty());
    }
    #[test]
    fn test_anomaly_type_description() {
        assert!(!AnomalyType::MemorySpike.description().is_empty());
        assert!(!AnomalyType::MemoryLeak.description().is_empty());
        assert!(!AnomalyType::CpuSpike.description().is_empty());
        assert!(!AnomalyType::NetworkSpike.description().is_empty());
        assert!(!AnomalyType::StorageSpike.description().is_empty());
        assert!(!AnomalyType::ActivityDrop.description().is_empty());
    }
    #[test]
    fn test_anomaly_config_presets() {
        let default = AnomalyConfig::default();
        assert_eq!(default.memory_spike_threshold, 2.0);
        let strict = AnomalyConfig::strict();
        assert_eq!(strict.memory_spike_threshold, 1.5);
        let lenient = AnomalyConfig::lenient();
        assert_eq!(lenient.memory_spike_threshold, 3.0);
    }
    #[test]
    fn test_anomaly_detector_creation() {
        let detector = AnomalyDetector::new(AnomalyConfig::default());
        assert!(detector.anomalies().is_empty());
    }
    #[test]
    fn test_anomaly_detector_memory_spike() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..5 {
            let usage = ResourceUsage {
                total_memory_bytes: 1000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        let usage = ResourceUsage {
            total_memory_bytes: 5000,
            ..Default::default()
        };
        history.record(&usage, 5_000_000);
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        let anomalies = detector.analyze(&history, 5_000_000);
        assert!(!anomalies.is_empty());
        assert!(anomalies
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::MemorySpike));
    }
    #[test]
    fn test_anomaly_detector_memory_leak() {
        let agent_id = [1u8; 16];
        let config = AnomalyConfig {
            leak_detection_samples: 5,
            leak_growth_threshold: 0.8,
            ..Default::default()
        };
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        let mut detector = AnomalyDetector::new(config);
        for i in 0..10 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        let anomalies = detector.analyze(&history, 10_000_000);
        assert!(anomalies
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::MemoryLeak));
    }
    #[test]
    fn test_anomaly_detector_filter_by_type() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..3 {
            let usage = ResourceUsage {
                total_memory_bytes: 1000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        history.record(
            &ResourceUsage {
                total_memory_bytes: 5000,
                ..Default::default()
            },
            3_000_000,
        );
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.analyze(&history, 3_000_000);
        let memory_anomalies = detector.anomalies_of_type(AnomalyType::MemorySpike);
        assert!(!memory_anomalies.is_empty());
        let network_anomalies = detector.anomalies_of_type(AnomalyType::NetworkSpike);
        assert!(network_anomalies.is_empty());
    }
    #[test]
    fn test_anomaly_detector_time_range() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for t in [1_000_000u64, 2_000_000, 3_000_000] {
            for i in 0..3 {
                history.record(
                    &ResourceUsage {
                        total_memory_bytes: 1000,
                        ..Default::default()
                    },
                    t - (3 - i) * 100_000,
                );
            }
            history.record(
                &ResourceUsage {
                    total_memory_bytes: 5000,
                    ..Default::default()
                },
                t,
            );
            detector.analyze(&history, t);
        }
        let in_range = detector.anomalies_in_range(1_500_000, 2_500_000);
        assert!(!in_range.is_empty());
    }
    #[test]
    fn test_anomaly_detector_high_severity() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..3 {
            history.record(
                &ResourceUsage {
                    total_memory_bytes: 1000,
                    ..Default::default()
                },
                i * 1_000_000,
            );
        }
        history.record(
            &ResourceUsage {
                total_memory_bytes: 10000,
                ..Default::default()
            },
            3_000_000,
        );
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.analyze(&history, 3_000_000);
        let high_severity = detector.high_severity_anomalies(0.5);
        assert!(!high_severity.is_empty());
    }
    #[test]
    fn test_anomaly_detector_clear() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..3 {
            history.record(
                &ResourceUsage {
                    total_memory_bytes: 1000,
                    ..Default::default()
                },
                i * 1_000_000,
            );
        }
        history.record(
            &ResourceUsage {
                total_memory_bytes: 5000,
                ..Default::default()
            },
            3_000_000,
        );
        detector.analyze(&history, 3_000_000);
        assert!(!detector.anomalies().is_empty());
        detector.clear();
        assert!(detector.anomalies().is_empty());
    }
    #[test]
    fn test_resource_predictor_creation() {
        let predictor = ResourcePredictor::new();
        assert_eq!(predictor.min_samples, 5);
        let predictor2 = ResourcePredictor::new().with_min_samples(10);
        assert_eq!(predictor2.min_samples, 10);
    }
    #[test]
    fn test_resource_predictor_insufficient_samples() {
        let agent_id = [1u8; 16];
        let history = ResourceHistory::new(agent_id, HistoryConfig::default());
        let predictor = ResourcePredictor::new();
        assert!(predictor.predict(&history, 1_000_000).is_none());
    }
    #[test]
    fn test_resource_predictor_predict() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..10 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1000,
                total_cpu_time_us: i * 100_000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        let predictor = ResourcePredictor::new();
        let prediction = predictor.predict(&history, 1_000_000).unwrap();
        assert!(prediction.memory_bytes >= 10000);
        assert!(prediction.confidence > 0.0);
        assert_eq!(prediction.horizon_us, 1_000_000);
    }
    #[test]
    fn test_resource_predictor_quota_breach() {
        let agent_id = [1u8; 16];
        let mut history = ResourceHistory::new(agent_id, HistoryConfig::default());
        for i in 0..10 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1_000_000,
                ..Default::default()
            };
            history.record(&usage, i * 1_000_000);
        }
        let quota = ResourceQuota::minimal();
        let predictor = ResourcePredictor::new();
        let breach_time = predictor.predict_quota_breach(&history, &quota);
        assert!(breach_time.is_some());
    }
    #[test]
    fn test_resource_monitor_creation() {
        let agent_id = [1u8; 16];
        let monitor = ResourceMonitor::new(agent_id);
        assert_eq!(monitor.agent_id(), &agent_id);
        assert!(monitor.history().is_empty());
        assert!(monitor.detector().anomalies().is_empty());
    }
    #[test]
    fn test_resource_monitor_with_config() {
        let agent_id = [1u8; 16];
        let monitor = ResourceMonitor::with_config(
            agent_id,
            HistoryConfig::high_resolution(),
            AnomalyConfig::strict(),
        );
        assert_eq!(monitor.detector().config().memory_spike_threshold, 1.5);
    }
    #[test]
    fn test_resource_monitor_record_and_analyze() {
        let agent_id = [1u8; 16];
        let mut monitor = ResourceMonitor::new(agent_id);
        for i in 0..5 {
            let usage = ResourceUsage {
                total_memory_bytes: 1000,
                ..Default::default()
            };
            monitor.record_and_analyze(&usage, i * 1_000_000);
        }
        let usage = ResourceUsage {
            total_memory_bytes: 5000,
            ..Default::default()
        };
        let anomalies = monitor.record_and_analyze(&usage, 5_000_000);
        assert!(!anomalies.is_empty());
        assert_eq!(monitor.history().len(), 6);
    }
    #[test]
    fn test_resource_monitor_predict() {
        let agent_id = [1u8; 16];
        let mut monitor = ResourceMonitor::new(agent_id);
        for i in 0..10 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1000,
                ..Default::default()
            };
            monitor.record_and_analyze(&usage, i * 1_000_000);
        }
        let prediction = monitor.predict(1_000_000);
        assert!(prediction.is_some());
    }
    #[test]
    fn test_resource_monitor_summary() {
        let agent_id = [1u8; 16];
        let mut monitor = ResourceMonitor::new(agent_id);
        for i in 0..5 {
            let usage = ResourceUsage {
                total_memory_bytes: (i + 1) * 1000,
                total_cpu_time_us: i * 100,
                ..Default::default()
            };
            monitor.record_and_analyze(&usage, i * 1_000_000);
        }
        let summary = monitor.summary();
        assert_eq!(summary.agent_id, agent_id);
        assert_eq!(summary.snapshot_count, 5);
        assert_eq!(summary.peak_memory_bytes, 5000);
        assert!(summary.latest_snapshot.is_some());
    }
    #[test]
    fn test_resource_monitor_clear() {
        let agent_id = [1u8; 16];
        let mut monitor = ResourceMonitor::new(agent_id);
        for i in 0..3 {
            monitor.record_and_analyze(&ResourceUsage::default(), i * 1_000_000);
        }
        assert!(!monitor.history().is_empty());
        monitor.clear();
        assert!(monitor.history().is_empty());
    }
}
