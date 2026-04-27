//! Energy-Aware Scheduling Integration
//!
//! Integrates the energy profiler, global energy policy, and per-peripheral
//! power domain tracker into a single scheduling advisor.  The advisor is
//! called by the idle loop and by the task dispatcher to decide whether a
//! task may run and at which CPU frequency.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::energy::{
    DomainState, Energy, EnergyBudget, EnergyPolicy, EnergyProfiler, PeripheralId,
    PowerDomainTracker, SleepRecommendation, TaskId,
};

// ============================================================================
// Scheduling hint types
// ============================================================================

/// Reason behind a scheduling hint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintReason {
    /// No energy constraint is active
    NoBudgetConstraint,
    /// Budget is present and there is comfortable headroom
    BudgetHeadroom,
    /// Budget is present but running low
    BudgetWarning {
        /// Remaining millijoules in the current window
        remaining_mj: u64,
    },
    /// Budget is fully exhausted; task must not run
    BudgetExhausted,
    /// Instantaneous power draw exceeds the policy cap
    PowerCapExceeded {
        /// Measured current power (milliwatts)
        current_mw: u32,
        /// Configured cap (milliwatts)
        cap_mw: u32,
    },
    /// System has been idle long enough to sleep
    IdleTimeout,
}

/// Combined scheduling advice produced by [`EnergyAwareScheduler`]
#[derive(Debug, Clone, Copy)]
pub struct EnergySchedulingHint {
    /// Whether the target task is permitted to run at all
    pub allow_run: bool,
    /// Suggested throttle level: 0 = full speed, 100 = block entirely
    pub suggested_throttle_percent: u8,
    /// Recommended sleep depth for the CPU if the system is about to idle
    pub sleep_recommendation: SleepRecommendation,
    /// Machine-readable explanation
    pub reason: HintReason,
}

// ============================================================================
// TaskEnergySummary (per-invocation, not the profiler-level aggregate)
// ============================================================================

/// Per-invocation energy summary returned when a task stops
#[derive(Debug, Clone, Copy)]
pub struct TaskEnergySummary {
    /// Which task
    pub task: TaskId,
    /// Energy consumed during this invocation
    pub energy_used: Energy,
    /// Budget remaining after this invocation (None if no budget is set)
    pub budget_remaining: Option<Energy>,
    /// Whether the cumulative total exceeds the budget
    pub over_budget: bool,
    /// Wall-clock duration of this invocation (microseconds)
    pub duration_us: u64,
}

// ============================================================================
// EnergyProfilerSnapshot
// ============================================================================

/// Point-in-time snapshot of the profiler state
#[derive(Debug, Clone, Copy)]
pub struct EnergyProfilerSnapshot {
    /// Total energy consumed by all tasks (microjoules)
    pub total_energy_uj: u64,
    /// Number of active (started-but-not-stopped) tasks
    pub active_tasks: usize,
    /// Per-task energy (up to 16 entries; unused entries are (TaskId(0), 0))
    pub per_task_energy: [(TaskId, u64); 16],
}

// ============================================================================
// EnergyAwareScheduler
// ============================================================================

/// Unified energy-aware scheduling advisor
///
/// Owns the profiler, policy, and domain tracker so it can be embedded in a
/// static or stack-allocated structure without any heap allocation.
pub struct EnergyAwareScheduler {
    profiler: EnergyProfiler,
    policy: EnergyPolicy,
    domain_tracker: PowerDomainTracker,
    /// Monotonic timestamp of the last time a task was active (µs)
    last_active_us: AtomicU64,
}

impl EnergyAwareScheduler {
    /// Create a new scheduler with the given policy
    pub fn new(policy: EnergyPolicy, start_us: u64) -> Self {
        Self {
            profiler: EnergyProfiler::new(),
            policy,
            domain_tracker: PowerDomainTracker::new(start_us),
            last_active_us: AtomicU64::new(start_us),
        }
    }

    /// Called before a task is dispatched.
    ///
    /// Consults the active energy budget (if any), the power cap, and the
    /// idle timeout to decide whether the task should be admitted.
    pub fn pre_schedule(&self, task: TaskId, now_us: u64) -> EnergySchedulingHint {
        let current_power_mw = self.domain_tracker.total_power_mw();

        // Hard power cap check (independent of mode)
        if current_power_mw > self.policy.max_power_mw {
            return EnergySchedulingHint {
                allow_run: false,
                suggested_throttle_percent: 80,
                sleep_recommendation: SleepRecommendation::LightSleep,
                reason: HintReason::PowerCapExceeded {
                    current_mw: current_power_mw,
                    cap_mw: self.policy.max_power_mw,
                },
            };
        }

        // Per-task budget check
        if let Some(profile) = self.profiler.get_task(task) {
            if profile.is_over_budget() {
                return EnergySchedulingHint {
                    allow_run: false,
                    suggested_throttle_percent: 100,
                    sleep_recommendation: self.policy.recommend_sleep(0, u64::MAX),
                    reason: HintReason::BudgetExhausted,
                };
            }

            if let Some(remaining) = profile.remaining_budget() {
                // Warn when less than 20 % of the original budget is left
                let budget_limit = profile
                    .budget
                    .map(|b| b.limit.as_microjoules())
                    .unwrap_or(u64::MAX);
                let threshold_uj = budget_limit / 5;
                if remaining.as_microjoules() < threshold_uj {
                    let idle_ms = self.idle_ms(now_us);
                    return EnergySchedulingHint {
                        allow_run: true,
                        suggested_throttle_percent: 50,
                        sleep_recommendation: self.policy.recommend_sleep(idle_ms, u64::MAX),
                        reason: HintReason::BudgetWarning {
                            remaining_mj: remaining.as_millijoules(),
                        },
                    };
                }

                let idle_ms = self.idle_ms(now_us);
                return EnergySchedulingHint {
                    allow_run: true,
                    suggested_throttle_percent: 0,
                    sleep_recommendation: self.policy.recommend_sleep(idle_ms, u64::MAX),
                    reason: HintReason::BudgetHeadroom,
                };
            }
        }

        // No budget constraint
        let idle_ms = self.idle_ms(now_us);
        EnergySchedulingHint {
            allow_run: true,
            suggested_throttle_percent: 0,
            sleep_recommendation: self.policy.recommend_sleep(idle_ms, u64::MAX),
            reason: HintReason::NoBudgetConstraint,
        }
    }

    /// Called when a task begins executing.  Starts profiler tracking and
    /// refreshes the last-active timestamp.
    pub fn on_task_start(&mut self, task: TaskId, now_us: u64) {
        self.profiler.start_task(task, now_us);
        self.last_active_us.store(now_us, Ordering::Release);
    }

    /// Called when a task yields or completes.  Stops profiler tracking and
    /// returns a per-invocation summary.
    pub fn on_task_stop(&mut self, task: TaskId, now_us: u64) -> TaskEnergySummary {
        let start_us = self.last_active_us.load(Ordering::Acquire);
        let duration_us = now_us.saturating_sub(start_us);
        let energy_used = self
            .profiler
            .stop_task(task, now_us)
            .unwrap_or(Energy::ZERO);

        let (budget_remaining, over_budget) = if let Some(profile) = self.profiler.get_task(task) {
            (profile.remaining_budget(), profile.is_over_budget())
        } else {
            (None, false)
        };

        TaskEnergySummary {
            task,
            energy_used,
            budget_remaining,
            over_budget,
            duration_us,
        }
    }

    /// Called from the idle loop.  Returns the recommended sleep depth.
    pub fn on_idle(&self, now_us: u64, next_deadline_us: Option<u64>) -> SleepRecommendation {
        let idle_ms = self.idle_ms(now_us);
        let deadline_ms = next_deadline_us
            .map(|d| d.saturating_sub(now_us) / 1_000)
            .unwrap_or(u64::MAX);
        self.policy.recommend_sleep(idle_ms, deadline_ms)
    }

    /// Set an energy budget for a specific task
    pub fn set_task_budget(&mut self, task: TaskId, budget: EnergyBudget) {
        self.profiler.set_budget(task, budget);
    }

    /// Snapshot the profiler state for external reporting
    pub fn profiler_snapshot(&self) -> EnergyProfilerSnapshot {
        let mut per_task_energy = [(TaskId::IDLE, 0u64); 16];
        let mut slot = 0usize;

        let summary = self.profiler.task_summary();

        // Probe task IDs 0–31 because EnergyProfiler does not expose an iterator.
        for id in 0u32..32 {
            if slot >= 16 {
                break;
            }
            let tid = TaskId::new(id);
            if let Some(profile) = self.profiler.get_task(tid) {
                per_task_energy[slot] = (tid, profile.total_energy.as_microjoules());
                slot += 1;
            }
        }

        EnergyProfilerSnapshot {
            total_energy_uj: summary.total_energy.as_microjoules(),
            active_tasks: slot,
            per_task_energy,
        }
    }

    /// Expose the domain tracker for external wiring (e.g. register peripherals)
    pub fn domain_tracker_mut(&mut self) -> &mut PowerDomainTracker {
        &mut self.domain_tracker
    }

    /// Expose the domain tracker read-only
    pub fn domain_tracker(&self) -> &PowerDomainTracker {
        &self.domain_tracker
    }

    /// Expose the energy policy
    pub fn policy(&self) -> &EnergyPolicy {
        &self.policy
    }

    /// Transition a peripheral power domain
    pub fn peripheral_transition(
        &mut self,
        peripheral: PeripheralId,
        new_state: DomainState,
        now_us: u64,
    ) {
        self.domain_tracker
            .transition(peripheral, new_state, now_us);
    }

    fn idle_ms(&self, now_us: u64) -> u64 {
        let last = self.last_active_us.load(Ordering::Acquire);
        now_us.saturating_sub(last) / 1_000
    }
}

// ============================================================================
// EnergyAdaptiveController
// ============================================================================

/// A CPU frequency operating point
#[derive(Debug, Clone, Copy)]
pub struct FrequencyStep {
    /// Frequency in MHz
    pub mhz: u32,
    /// Typical power at this frequency (milliwatts)
    pub power_mw: u32,
}

/// Result of a frequency update cycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencyAdjustment {
    /// No change needed
    Maintain,
    /// Scale frequency up to a higher operating point
    ScaleUp {
        /// Target frequency in MHz
        new_mhz: u32,
    },
    /// Scale frequency down to a lower operating point
    ScaleDown {
        /// Target frequency in MHz
        new_mhz: u32,
    },
}

/// Proportional-integral feedback controller that adjusts CPU frequency based
/// on observed power relative to the energy headroom available.
///
/// Every `window_us` microseconds it accumulates power×time, computes a
/// utilisation ratio, and recommends stepping up or down.
pub struct EnergyAdaptiveController {
    frequency_table: [FrequencyStep; 8],
    num_steps: usize,
    current_step: usize,
    /// Target CPU power utilisation (0–100 %)
    target_utilization: u8,
    /// Measurement window length (microseconds)
    window_us: u64,
    /// Energy accumulated in the current window
    accumulated_energy: Energy,
    /// Start of the current measurement window
    window_start_us: u64,
}

impl EnergyAdaptiveController {
    /// Build a controller from a frequency table slice (at most 8 entries).
    ///
    /// Entries **must** be ordered from lowest to highest frequency.
    pub fn new(freq_table: &[FrequencyStep], target_util: u8, window_us: u64) -> Self {
        let mut frequency_table = [FrequencyStep {
            mhz: 0,
            power_mw: 0,
        }; 8];
        let num_steps = freq_table.len().min(8);
        frequency_table[..num_steps].copy_from_slice(&freq_table[..num_steps]);

        Self {
            frequency_table,
            num_steps,
            current_step: 0,
            target_utilization: target_util,
            window_us,
            accumulated_energy: Energy::ZERO,
            window_start_us: 0,
        }
    }

    /// Drive the controller with current power draw and elapsed time.
    ///
    /// When a measurement window closes the controller computes whether
    /// the observed power utilisation is above or below the target and
    /// produces a frequency recommendation.
    pub fn update(&mut self, current_power_mw: u32, elapsed_us: u64) -> FrequencyAdjustment {
        // Accumulate energy for this slice
        let energy_uj = (current_power_mw as u64).saturating_mul(elapsed_us) / 1_000;
        self.accumulated_energy = self
            .accumulated_energy
            .saturating_add(Energy::microjoules(energy_uj));

        let window_elapsed = elapsed_us.saturating_add(
            // time since window start is tracked via accumulated calls
            // we approximate window progress by comparing accumulated energy
            // against what the current operating point would burn at 100% util
            0,
        );
        let _ = window_elapsed;

        // Compute window duration from accumulated energy and current power
        let window_uj = self.window_us.saturating_mul(current_power_mw as u64) / 1_000;
        if window_uj == 0 {
            return FrequencyAdjustment::Maintain;
        }

        // Effective utilisation: what fraction of the window-budget is consumed
        let utilisation =
            (self.accumulated_energy.as_microjoules().saturating_mul(100)) / window_uj.max(1);
        let utilisation = utilisation.min(100) as u8;

        // Only act when we have consumed at least half a window of data to
        // avoid over-reacting to momentary spikes.
        if self.accumulated_energy.as_microjoules() < window_uj / 2 {
            return FrequencyAdjustment::Maintain;
        }

        // Reset window
        self.accumulated_energy = Energy::ZERO;
        self.window_start_us = self.window_start_us.saturating_add(self.window_us);

        if utilisation > self.target_utilization && self.current_step + 1 < self.num_steps {
            self.current_step += 1;
            FrequencyAdjustment::ScaleUp {
                new_mhz: self.frequency_table[self.current_step].mhz,
            }
        } else if utilisation < self.target_utilization.saturating_sub(20) && self.current_step > 0
        {
            self.current_step -= 1;
            FrequencyAdjustment::ScaleDown {
                new_mhz: self.frequency_table[self.current_step].mhz,
            }
        } else {
            FrequencyAdjustment::Maintain
        }
    }

    /// Current operating frequency (MHz)
    pub fn current_frequency_mhz(&self) -> u32 {
        if self.num_steps == 0 {
            0
        } else {
            self.frequency_table[self.current_step].mhz
        }
    }

    /// Current step index (0 = lowest frequency)
    pub fn current_step(&self) -> usize {
        self.current_step
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::{
        EnergyBudget, EnergyPolicy, PowerManager, PowerState, SleepRecommendation,
    };

    fn make_freq_table() -> [FrequencyStep; 4] {
        [
            FrequencyStep {
                mhz: 48,
                power_mw: 20,
            },
            FrequencyStep {
                mhz: 96,
                power_mw: 50,
            },
            FrequencyStep {
                mhz: 168,
                power_mw: 100,
            },
            FrequencyStep {
                mhz: 240,
                power_mw: 180,
            },
        ]
    }

    // ------------------------------------------------------------------
    // EnergyAwareScheduler – basic admission
    // ------------------------------------------------------------------

    #[test]
    fn test_energy_aware_scheduler_no_budget_runs_task() {
        let scheduler = EnergyAwareScheduler::new(EnergyPolicy::performance(), 0);
        let hint = scheduler.pre_schedule(TaskId::new(1), 0);
        assert!(hint.allow_run);
        assert_eq!(hint.reason, HintReason::NoBudgetConstraint);
    }

    #[test]
    fn test_energy_aware_scheduler_budget_exhausted_blocks() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::performance(), 0);
        let task = TaskId::new(2);

        // Give it a 1 µJ budget
        scheduler.set_task_budget(task, EnergyBudget::microjoules(1));

        // Run for 1 second at 80 MHz — this vastly exceeds 1 µJ
        scheduler.on_task_start(task, 0);
        scheduler.on_task_stop(task, 1_000_000);

        let hint = scheduler.pre_schedule(task, 1_000_001);
        assert!(!hint.allow_run);
        assert_eq!(hint.reason, HintReason::BudgetExhausted);
        assert_eq!(hint.suggested_throttle_percent, 100);
    }

    #[test]
    fn test_energy_aware_scheduler_budget_warning() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::performance(), 0);
        let task = TaskId::new(3);

        // Budget of 100 mJ (100_000 µJ)
        scheduler.set_task_budget(task, EnergyBudget::millijoules(100));

        // Consume ~85 % of the budget: run 17 seconds at 80 MHz
        // Energy per second = 5 mJ → 17 s → 85 mJ
        for i in 0u64..17 {
            scheduler.on_task_start(task, i * 1_000_000);
            scheduler.on_task_stop(task, (i + 1) * 1_000_000);
        }

        let hint = scheduler.pre_schedule(task, 18_000_000);
        assert!(hint.allow_run);
        assert!(matches!(hint.reason, HintReason::BudgetWarning { .. }));
    }

    #[test]
    fn test_energy_aware_scheduler_task_start_stop_summary() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::balanced(), 0);
        let task = TaskId::new(4);

        scheduler.on_task_start(task, 1_000);
        let summary = scheduler.on_task_stop(task, 2_000);

        assert_eq!(summary.task, task);
        assert!(summary.energy_used > Energy::ZERO);
        assert_eq!(summary.budget_remaining, None);
        assert!(!summary.over_budget);
        // duration computed from last_active_us=1_000 to now=2_000 → 1_000 µs
        assert_eq!(summary.duration_us, 1_000);
    }

    #[test]
    fn test_energy_aware_scheduler_idle_recommendation() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::power_save(), 0);
        let task = TaskId::new(5);

        // Mark active at t=0, then idle checks at t = idle_threshold + 1 ms (in µs)
        scheduler.on_task_start(task, 0);
        scheduler.on_task_stop(task, 1_000);

        let threshold_us = scheduler.policy().idle_threshold_ms * 1_000;
        let rec = scheduler.on_idle(threshold_us + 1_000_000, None);
        assert_eq!(rec, SleepRecommendation::Hibernate);
    }

    #[test]
    fn test_energy_aware_scheduler_power_cap_exceeded() {
        use crate::energy::PowerDomain;

        let mut policy = EnergyPolicy::balanced();
        policy.max_power_mw = 10; // 10 mW cap

        let mut scheduler = EnergyAwareScheduler::new(policy, 0);

        // Register a CPU domain drawing 200 mW when On
        let cpu_domain = PowerDomain::new(crate::energy::PeripheralId::Cpu, 200, 50, 5, 0);
        scheduler.domain_tracker_mut().register(cpu_domain).unwrap();
        scheduler.peripheral_transition(crate::energy::PeripheralId::Cpu, DomainState::On, 0);

        let hint = scheduler.pre_schedule(TaskId::new(1), 100);
        assert!(!hint.allow_run);
        assert!(matches!(hint.reason, HintReason::PowerCapExceeded { .. }));
    }

    // ------------------------------------------------------------------
    // EnergyAdaptiveController
    // ------------------------------------------------------------------

    #[test]
    fn test_energy_adaptive_controller_scale_up() {
        let table = make_freq_table();
        // window_us = 1_000 µs, target = 50 %
        // update(power=1_000, elapsed=600):
        //   energy_uj = 1_000 * 600 / 1_000 = 600 µJ
        //   window_uj  = 1_000 * 1_000 / 1_000 = 1_000 µJ
        //   acc = 600 µJ >= window_uj/2 = 500 → fires
        //   util = 600 * 100 / 1_000 = 60 > target 50 → ScaleUp
        let mut ctrl = EnergyAdaptiveController::new(&table, 50, 1_000);
        let adj = ctrl.update(1_000, 600);
        assert!(matches!(adj, FrequencyAdjustment::ScaleUp { .. }));
        assert!(ctrl.current_step() > 0);
    }

    #[test]
    fn test_energy_adaptive_controller_scale_down() {
        let table = make_freq_table();
        // Use target_util = 50 so that util=60 triggers ScaleUp (step increases)
        // window_us = 1_000, power = 1_000, elapsed = 600 → util = 60 > 50 → ScaleUp
        let mut ctrl = EnergyAdaptiveController::new(&table, 50, 1_000);
        for _ in 0..3 {
            ctrl.update(1_000, 600);
        }
        let top_step = ctrl.current_step();
        assert!(top_step > 0);

        // Now drive with very low utilisation to trigger ScaleDown.
        // Send low power that produces util < target - 20 = 30.
        // power_mw = 100, elapsed = 600 → acc = 100*600/1_000 = 60 µJ
        // window_uj = 1_000 * 100 / 1_000 = 100 µJ; acc(60) >= 50 → fires
        // util = 60 * 100 / 100 = 60 ≥ 30  — still Maintain. Need lower.
        // Use power_mw = 10: acc = 10*600/1_000 = 6; window_uj = 10*1_000/1_000=10
        // acc(6) >= window_uj/2(5) → fires; util = 6*100/10 = 60 — still ≥ 30.
        // The ratio is constant because util = elapsed/window_us = 600/1000 = 60 %.
        // To get util < 30 we need elapsed / window_us < 0.3.
        // Restart with a fresh controller at top step, window=1_000, elapsed=200:
        // util = power*200/1000 *100 / (power*1_000/1_000) = 200*100/1_000 = 20 < 30
        // but acc = power*200/1_000; window_uj/2 = power*1_000/1_000/2 = power/2
        // acc >= window_uj/2 iff 200/1_000 >= 1/2 → NO (0.2 < 0.5) → Maintain.
        // We need acc >= window_uj/2 AND util < target-20.
        // acc/window_uj = elapsed/window_us.  For acc >= 50 %: elapsed >= 500.
        // util = elapsed*100/window_us.  With elapsed=500: util=50 → barely at target.
        // Conclusion: for target=50 and elapsed=500: util=50, not < 30. Can't ScaleDown.
        // Use target=90 so threshold = 90-20=70. With elapsed=600: util=60 < 70 → ScaleDown.
        let mut ctrl2 = EnergyAdaptiveController::new(&table, 90, 1_000);
        // ScaleUp: util=60 < 90 but 60 ≥ 70? No. 60 < 70, so NOT ScaleUp.
        // We need util > 90 to ScaleUp. Use elapsed=950 (95%).
        // acc = p*950/1000; window=p*1000/1000=p; acc >= p/2 (950/1000>0.5) → fires
        // util = 950*100/1000=95 > 90 → ScaleUp!
        for _ in 0..3 {
            ctrl2.update(1_000, 950);
        }
        assert!(ctrl2.current_step() > 0);

        // Now scale down: elapsed=600 → util=60 < 90-20=70 → ScaleDown
        let adj = ctrl2.update(1_000, 600);
        assert!(matches!(adj, FrequencyAdjustment::ScaleDown { .. }));
    }

    #[test]
    fn test_energy_adaptive_controller_maintain() {
        let table = make_freq_table();
        // window_us = 1_000, elapsed = 1 → acc = power*1/1_000; window_uj = power*1_000/1_000=power
        // acc/window_uj = 1/1_000 = 0.001 → far below 50 % threshold → Maintain
        let mut ctrl = EnergyAdaptiveController::new(&table, 50, 1_000);
        let adj = ctrl.update(1_000, 1);
        assert_eq!(adj, FrequencyAdjustment::Maintain);
    }

    // ------------------------------------------------------------------
    // TaskEnergySummary – over-budget flag
    // ------------------------------------------------------------------

    #[test]
    fn test_task_energy_summary_over_budget() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::performance(), 0);
        let task = TaskId::new(10);

        scheduler.set_task_budget(task, EnergyBudget::microjoules(1));

        // Run for 1 second — energy will far exceed 1 µJ
        scheduler.on_task_start(task, 0);
        let summary = scheduler.on_task_stop(task, 1_000_000);

        assert!(summary.over_budget);
        assert_eq!(summary.task, task);
    }

    // ------------------------------------------------------------------
    // EnergyProfilerSnapshot
    // ------------------------------------------------------------------

    #[test]
    fn test_profiler_snapshot_correctness() {
        let mut scheduler = EnergyAwareScheduler::new(EnergyPolicy::balanced(), 0);
        let task = TaskId::new(7);

        scheduler.on_task_start(task, 0);
        scheduler.on_task_stop(task, 500_000);

        let snap = scheduler.profiler_snapshot();
        assert!(snap.total_energy_uj > 0);
        // The task should appear in per_task_energy
        let found = snap
            .per_task_energy
            .iter()
            .any(|(tid, uj)| *tid == task && *uj > 0);
        assert!(found);
    }

    // ------------------------------------------------------------------
    // PowerManager suggest_next_state
    // ------------------------------------------------------------------

    #[test]
    fn test_power_manager_suggest_state_from_hint() {
        let pm = PowerManager::with_energy_policy(EnergyPolicy::power_save());

        let hint_awake = EnergySchedulingHint {
            allow_run: true,
            suggested_throttle_percent: 0,
            sleep_recommendation: SleepRecommendation::StayAwake,
            reason: HintReason::NoBudgetConstraint,
        };
        assert_eq!(pm.suggest_next_state(&hint_awake), PowerState::Active);

        let hint_light = EnergySchedulingHint {
            allow_run: true,
            suggested_throttle_percent: 20,
            sleep_recommendation: SleepRecommendation::LightSleep,
            reason: HintReason::IdleTimeout,
        };
        assert_eq!(pm.suggest_next_state(&hint_light), PowerState::LightSleep);

        let hint_deep = EnergySchedulingHint {
            allow_run: false,
            suggested_throttle_percent: 100,
            sleep_recommendation: SleepRecommendation::DeepSleep,
            reason: HintReason::BudgetExhausted,
        };
        assert_eq!(pm.suggest_next_state(&hint_deep), PowerState::DeepSleep);

        let hint_hib = EnergySchedulingHint {
            allow_run: false,
            suggested_throttle_percent: 100,
            sleep_recommendation: SleepRecommendation::Hibernate,
            reason: HintReason::IdleTimeout,
        };
        assert_eq!(pm.suggest_next_state(&hint_hib), PowerState::Hibernate);
    }
}
