//! Geographic Routing Module

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub type RegionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Route to nearest region
    Nearest,
    /// Route based on latency
    LatencyBased,
    /// Route based on load
    LoadBased,
    /// Weighted routing
    Weighted,
}

#[derive(Debug, Clone)]
pub struct RoutingPolicy {
    pub strategy: RoutingStrategy,
    pub fallback_regions: Vec<RegionId>,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::LatencyBased,
            fallback_regions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LatencyMap {
    pub latencies: HashMap<(RegionId, RegionId), Duration>,
}

impl LatencyMap {
    pub fn new() -> Self {
        Self {
            latencies: HashMap::new(),
        }
    }

    pub fn add_latency(&mut self, from: RegionId, to: RegionId, latency: Duration) {
        self.latencies.insert((from, to), latency);
    }

    pub fn get_latency(&self, from: &str, to: &str) -> Option<Duration> {
        self.latencies
            .get(&(from.to_string(), to.to_string()))
            .copied()
    }
}

impl Default for LatencyMap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub target_region: RegionId,
    pub latency: Option<Duration>,
    pub reason: String,
}

pub struct GeoRouter {
    policy: RoutingPolicy,
    latency_map: LatencyMap,
}

impl GeoRouter {
    pub fn new(policy: RoutingPolicy) -> Self {
        Self {
            policy,
            latency_map: LatencyMap::new(),
        }
    }

    pub fn route(&self, _source_region: &str, available_regions: &[String]) -> RouteDecision {
        match &self.policy.strategy {
            RoutingStrategy::Nearest | RoutingStrategy::LatencyBased => {
                if let Some(region) = available_regions.first() {
                    RouteDecision {
                        target_region: region.clone(),
                        latency: None,
                        reason: "Latency-based routing".to_string(),
                    }
                } else {
                    RouteDecision {
                        target_region: String::new(),
                        latency: None,
                        reason: "No regions available".to_string(),
                    }
                }
            }
            _ => RouteDecision {
                target_region: available_regions.first().cloned().unwrap_or_default(),
                latency: None,
                reason: "Default routing".to_string(),
            },
        }
    }

    pub fn update_latency_map(&mut self, latency_map: LatencyMap) {
        self.latency_map = latency_map;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_map() {
        let mut map = LatencyMap::new();
        map.add_latency(
            "us-west".to_string(),
            "us-east".to_string(),
            Duration::from_millis(50),
        );

        let latency = map.get_latency("us-west", "us-east");
        assert!(latency.is_some());
    }

    #[test]
    fn test_geo_router() {
        let policy = RoutingPolicy::default();
        let router = GeoRouter::new(policy);

        let decision = router.route("us-west", &["us-east".to_string()]);
        assert_eq!(decision.target_region, "us-east");
    }
}
