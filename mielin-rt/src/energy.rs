//! Energy Profiling for Embedded Devices
//!
//! Provides per-task energy tracking, budget management, energy policy enforcement,
//! per-peripheral power domain accounting, and energy-aware scheduling hints for
//! embedded systems.
//!
//! ## Features
//!
//! - **Per-Task Energy Tracking**: Track energy consumed by individual tasks
//! - **Energy Budget Management**: Set and enforce energy budgets
//! - **Power Mode Statistics**: Track time spent in each power mode
//! - **Energy-Aware Scheduling**: Hints for energy-efficient task scheduling
//! - **Energy Policy**: Global device policy (performance/balanced/power-save/budget)
//! - **Power Domain Tracking**: Per-peripheral active/idle/off energy accounting
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::energy::{EnergyProfiler, TaskId, EnergyBudget};
//!
//! let mut profiler = EnergyProfiler::new();
//!
//! // Set energy budget for a task
//! let task = TaskId(1);
//! profiler.set_budget(task, EnergyBudget::millijoules(100));
//!
//! // Start tracking
//! profiler.start_task(task, 1000); // timestamp in microseconds
//!
//! // ... task runs ...
//!
//! // Stop tracking
//! profiler.stop_task(task, 1500); // timestamp in microseconds
//! ```

/// Task identifier for energy tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u32);

impl TaskId {
    /// System idle task
    pub const IDLE: Self = TaskId(0);

    /// Create a new task ID
    pub const fn new(id: u32) -> Self {
        TaskId(id)
    }

    /// Get the raw task ID
    pub const fn raw(&self) -> u32 {
        self.0
    }
}

/// Energy unit in microjoules (µJ)
///
/// Using microjoules provides good precision for embedded systems
/// where energy consumption is typically measured in millijoules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Energy(u64);

impl Energy {
    /// Zero energy
    pub const ZERO: Self = Energy(0);

    /// Create energy from microjoules
    pub const fn microjoules(uj: u64) -> Self {
        Energy(uj)
    }

    /// Create energy from millijoules
    pub const fn millijoules(mj: u64) -> Self {
        Energy(mj * 1000)
    }

    /// Create energy from joules
    pub const fn joules(j: u64) -> Self {
        Energy(j * 1_000_000)
    }

    /// Get energy in microjoules
    pub const fn as_microjoules(&self) -> u64 {
        self.0
    }

    /// Get energy in millijoules
    pub const fn as_millijoules(&self) -> u64 {
        self.0 / 1000
    }

    /// Saturating addition
    pub const fn saturating_add(self, other: Self) -> Self {
        Energy(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction
    pub const fn saturating_sub(self, other: Self) -> Self {
        Energy(self.0.saturating_sub(other.0))
    }
}

impl core::ops::Add for Energy {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Energy(self.0 + other.0)
    }
}

impl core::ops::AddAssign for Energy {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl core::ops::Sub for Energy {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Energy(self.0 - other.0)
    }
}

/// Energy budget for a task
#[derive(Debug, Clone, Copy)]
pub struct EnergyBudget {
    /// Maximum energy allowed
    pub limit: Energy,
    /// Action to take when budget is exceeded
    pub on_exceeded: BudgetAction,
}

impl EnergyBudget {
    /// Create a budget with the given limit in millijoules
    pub const fn millijoules(mj: u64) -> Self {
        Self {
            limit: Energy::millijoules(mj),
            on_exceeded: BudgetAction::Throttle,
        }
    }

    /// Create a budget with the given limit in microjoules
    pub const fn microjoules(uj: u64) -> Self {
        Self {
            limit: Energy::microjoules(uj),
            on_exceeded: BudgetAction::Throttle,
        }
    }

    /// Set the action to take when budget is exceeded
    pub const fn with_action(mut self, action: BudgetAction) -> Self {
        self.on_exceeded = action;
        self
    }
}

/// Action to take when energy budget is exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetAction {
    /// Reduce task priority/frequency
    #[default]
    Throttle,
    /// Suspend the task
    Suspend,
    /// Migrate the task to another node
    Migrate,
    /// Log but take no action
    LogOnly,
}

/// Power consumption model for different operations
#[derive(Debug, Clone, Copy)]
pub struct PowerModel {
    /// Base power in microwatts (when idle)
    pub idle_power: u32,
    /// CPU active power in microwatts (per MHz)
    pub cpu_power_per_mhz: u32,
    /// Memory access power in microwatts (per access)
    pub memory_power_per_access: u32,
    /// Radio transmit power in microwatts
    pub radio_tx_power: u32,
    /// Radio receive power in microwatts
    pub radio_rx_power: u32,
}

impl Default for PowerModel {
    fn default() -> Self {
        // Default values based on typical Cortex-M4 @ 80MHz
        Self {
            idle_power: 1_000,          // 1 mW idle
            cpu_power_per_mhz: 50,      // 50 µW per MHz
            memory_power_per_access: 1, // 1 µW per memory access
            radio_tx_power: 30_000,     // 30 mW transmit
            radio_rx_power: 15_000,     // 15 mW receive
        }
    }
}

impl PowerModel {
    /// Create a new power model with custom values
    pub const fn new(
        idle_power: u32,
        cpu_power_per_mhz: u32,
        memory_power_per_access: u32,
        radio_tx_power: u32,
        radio_rx_power: u32,
    ) -> Self {
        Self {
            idle_power,
            cpu_power_per_mhz,
            memory_power_per_access,
            radio_tx_power,
            radio_rx_power,
        }
    }

    /// Low power profile (e.g., Cortex-M0+)
    pub const fn low_power() -> Self {
        Self {
            idle_power: 500,       // 0.5 mW idle
            cpu_power_per_mhz: 25, // 25 µW per MHz
            memory_power_per_access: 1,
            radio_tx_power: 15_000, // 15 mW transmit
            radio_rx_power: 8_000,  // 8 mW receive
        }
    }

    /// High performance profile (e.g., Cortex-M7)
    pub const fn high_performance() -> Self {
        Self {
            idle_power: 5_000,      // 5 mW idle
            cpu_power_per_mhz: 100, // 100 µW per MHz
            memory_power_per_access: 2,
            radio_tx_power: 50_000, // 50 mW transmit
            radio_rx_power: 25_000, // 25 mW receive
        }
    }

    /// Estimate energy for CPU execution
    ///
    /// # Arguments
    /// * `duration_us` - Execution duration in microseconds
    /// * `frequency_mhz` - CPU frequency in MHz
    pub fn estimate_cpu_energy(&self, duration_us: u64, frequency_mhz: u32) -> Energy {
        // Energy = Power × Time
        // Power = idle + (cpu_power_per_mhz × frequency)
        let power_uw = self.idle_power + (self.cpu_power_per_mhz * frequency_mhz);
        // Energy in µJ = Power (µW) × Time (µs) / 1_000_000
        let energy_uj = (power_uw as u64 * duration_us) / 1_000_000;
        Energy::microjoules(energy_uj)
    }

    /// Estimate energy for memory operations
    pub fn estimate_memory_energy(&self, access_count: u64) -> Energy {
        let energy_uj = (self.memory_power_per_access as u64 * access_count) / 1000;
        Energy::microjoules(energy_uj)
    }

    /// Estimate energy for radio transmission
    pub fn estimate_radio_tx_energy(&self, duration_us: u64) -> Energy {
        let energy_uj = (self.radio_tx_power as u64 * duration_us) / 1_000_000;
        Energy::microjoules(energy_uj)
    }

    /// Estimate energy for radio reception
    pub fn estimate_radio_rx_energy(&self, duration_us: u64) -> Energy {
        let energy_uj = (self.radio_rx_power as u64 * duration_us) / 1_000_000;
        Energy::microjoules(energy_uj)
    }
}

/// Task energy profile tracking
#[derive(Debug, Clone, Default)]
pub struct TaskEnergyProfile {
    /// Task identifier
    pub task_id: Option<TaskId>,
    /// Total energy consumed
    pub total_energy: Energy,
    /// Energy budget (if set)
    pub budget: Option<EnergyBudget>,
    /// Start timestamp of current execution (microseconds)
    start_timestamp: Option<u64>,
    /// Execution count
    pub execution_count: u32,
    /// Total execution time in microseconds
    pub total_execution_time: u64,
}

impl TaskEnergyProfile {
    /// Create a new task energy profile
    pub fn new(task_id: TaskId) -> Self {
        Self {
            task_id: Some(task_id),
            ..Default::default()
        }
    }

    /// Set energy budget for this task
    pub fn set_budget(&mut self, budget: EnergyBudget) {
        self.budget = Some(budget);
    }

    /// Check if task has exceeded its budget
    pub fn is_over_budget(&self) -> bool {
        self.budget
            .as_ref()
            .is_some_and(|b| self.total_energy > b.limit)
    }

    /// Get remaining energy budget
    pub fn remaining_budget(&self) -> Option<Energy> {
        self.budget
            .as_ref()
            .map(|b| b.limit.saturating_sub(self.total_energy))
    }

    /// Get average energy per execution
    pub fn average_energy_per_execution(&self) -> Energy {
        if self.execution_count == 0 {
            Energy::ZERO
        } else {
            Energy::microjoules(self.total_energy.as_microjoules() / self.execution_count as u64)
        }
    }

    /// Get average execution time in microseconds
    pub fn average_execution_time(&self) -> u64 {
        if self.execution_count == 0 {
            0
        } else {
            self.total_execution_time / self.execution_count as u64
        }
    }
}

/// Statistics for power mode usage
#[derive(Debug, Clone, Default)]
pub struct PowerModeStats {
    /// Time spent in Normal mode (microseconds)
    pub normal_time: u64,
    /// Time spent in LowPower mode (microseconds)
    pub low_power_time: u64,
    /// Time spent in UltraLowPower mode (microseconds)
    pub ultra_low_power_time: u64,
    /// Time spent in Sleep mode (microseconds)
    pub sleep_time: u64,
    /// Total energy consumed in each mode
    pub normal_energy: Energy,
    pub low_power_energy: Energy,
    pub ultra_low_power_energy: Energy,
    pub sleep_energy: Energy,
    /// Current mode start timestamp
    mode_start_timestamp: u64,
    /// Current mode
    current_mode: Option<super::power::PowerMode>,
}

impl PowerModeStats {
    /// Create new power mode statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Start tracking a power mode
    pub fn start_mode(&mut self, mode: super::power::PowerMode, timestamp: u64) {
        // Record time spent in previous mode
        if let Some(prev_mode) = self.current_mode {
            let duration = timestamp.saturating_sub(self.mode_start_timestamp);
            self.record_mode_time(prev_mode, duration);
        }

        self.current_mode = Some(mode);
        self.mode_start_timestamp = timestamp;
    }

    /// Record time spent in a power mode
    fn record_mode_time(&mut self, mode: super::power::PowerMode, duration: u64) {
        match mode {
            super::power::PowerMode::Normal => self.normal_time += duration,
            super::power::PowerMode::LowPower => self.low_power_time += duration,
            super::power::PowerMode::UltraLowPower => self.ultra_low_power_time += duration,
            super::power::PowerMode::Sleep => self.sleep_time += duration,
            super::power::PowerMode::Standby => self.sleep_time += duration,
            super::power::PowerMode::Shutdown => {} // No time tracking during shutdown
        }
    }

    /// Get total tracked time
    pub fn total_time(&self) -> u64 {
        self.normal_time + self.low_power_time + self.ultra_low_power_time + self.sleep_time
    }

    /// Get percentage of time in each mode
    pub fn mode_percentages(&self) -> PowerModePercentages {
        let total = self.total_time();
        if total == 0 {
            return PowerModePercentages::default();
        }

        PowerModePercentages {
            normal: (self.normal_time * 100 / total) as u8,
            low_power: (self.low_power_time * 100 / total) as u8,
            ultra_low_power: (self.ultra_low_power_time * 100 / total) as u8,
            sleep: (self.sleep_time * 100 / total) as u8,
        }
    }

    /// Get total energy across all modes
    pub fn total_energy(&self) -> Energy {
        self.normal_energy + self.low_power_energy + self.ultra_low_power_energy + self.sleep_energy
    }

    /// Update energy consumption for current mode
    pub fn update_energy(&mut self, mode: super::power::PowerMode, energy: Energy) {
        match mode {
            super::power::PowerMode::Normal => self.normal_energy += energy,
            super::power::PowerMode::LowPower => self.low_power_energy += energy,
            super::power::PowerMode::UltraLowPower => self.ultra_low_power_energy += energy,
            super::power::PowerMode::Sleep => self.sleep_energy += energy,
            super::power::PowerMode::Standby => self.sleep_energy += energy,
            super::power::PowerMode::Shutdown => {} // No energy tracking during shutdown
        }
    }
}

/// Percentage breakdown of time in each power mode
#[derive(Debug, Clone, Default)]
pub struct PowerModePercentages {
    pub normal: u8,
    pub low_power: u8,
    pub ultra_low_power: u8,
    pub sleep: u8,
}

/// Scheduling hint based on energy analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingHint {
    /// Task can run at full speed
    FullSpeed,
    /// Task should be throttled to save energy
    Throttle { factor: u8 },
    /// Task should be delayed until more energy is available
    Delay { suggested_delay_ms: u32 },
    /// Task should be migrated to a lower-power node
    Migrate,
    /// Task should be suspended
    Suspend,
}

/// Maximum number of tasks to track
const MAX_TRACKED_TASKS: usize = 32;

/// Energy profiler for the system
#[derive(Debug)]
pub struct EnergyProfiler {
    /// Task energy profiles
    tasks: [Option<TaskEnergyProfile>; MAX_TRACKED_TASKS],
    /// Power model for energy estimation
    power_model: PowerModel,
    /// Power mode statistics
    mode_stats: PowerModeStats,
    /// CPU frequency in MHz
    cpu_frequency: u32,
    /// System energy budget
    system_budget: Option<Energy>,
    /// Total system energy consumed
    total_system_energy: Energy,
}

impl EnergyProfiler {
    /// Create a new energy profiler
    pub fn new() -> Self {
        Self {
            tasks: [const { None }; MAX_TRACKED_TASKS],
            power_model: PowerModel::default(),
            mode_stats: PowerModeStats::new(),
            cpu_frequency: 80, // Default 80 MHz
            system_budget: None,
            total_system_energy: Energy::ZERO,
        }
    }

    /// Create with a custom power model
    pub fn with_power_model(power_model: PowerModel) -> Self {
        Self {
            power_model,
            ..Self::new()
        }
    }

    /// Set CPU frequency for energy estimation
    pub fn set_cpu_frequency(&mut self, frequency_mhz: u32) {
        self.cpu_frequency = frequency_mhz;
    }

    /// Set system-wide energy budget
    pub fn set_system_budget(&mut self, budget: Energy) {
        self.system_budget = Some(budget);
    }

    /// Get the power model
    pub fn power_model(&self) -> &PowerModel {
        &self.power_model
    }

    /// Get power mode statistics
    pub fn mode_stats(&self) -> &PowerModeStats {
        &self.mode_stats
    }

    /// Get mutable power mode statistics
    pub fn mode_stats_mut(&mut self) -> &mut PowerModeStats {
        &mut self.mode_stats
    }

    /// Find the index of a task or an empty slot
    fn find_task_index(&self, task_id: TaskId) -> Option<usize> {
        for (i, slot) in self.tasks.iter().enumerate() {
            if let Some(ref profile) = slot {
                if profile.task_id == Some(task_id) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Find an empty slot index
    fn find_empty_slot(&self) -> Option<usize> {
        for (i, slot) in self.tasks.iter().enumerate() {
            if slot.is_none() {
                return Some(i);
            }
        }
        None
    }

    /// Find or create a task profile
    fn get_or_create_task(&mut self, task_id: TaskId) -> Option<&mut TaskEnergyProfile> {
        // First, try to find existing task
        if let Some(idx) = self.find_task_index(task_id) {
            return self.tasks[idx].as_mut();
        }

        // Not found, create new
        if let Some(idx) = self.find_empty_slot() {
            self.tasks[idx] = Some(TaskEnergyProfile::new(task_id));
            return self.tasks[idx].as_mut();
        }

        // No space available
        None
    }

    /// Get task profile (read-only)
    pub fn get_task(&self, task_id: TaskId) -> Option<&TaskEnergyProfile> {
        self.tasks
            .iter()
            .flatten()
            .find(|profile| profile.task_id == Some(task_id))
    }

    /// Set energy budget for a task
    pub fn set_budget(&mut self, task_id: TaskId, budget: EnergyBudget) -> bool {
        if let Some(profile) = self.get_or_create_task(task_id) {
            profile.set_budget(budget);
            true
        } else {
            false
        }
    }

    /// Start tracking task execution
    pub fn start_task(&mut self, task_id: TaskId, timestamp: u64) -> bool {
        if let Some(profile) = self.get_or_create_task(task_id) {
            profile.start_timestamp = Some(timestamp);
            true
        } else {
            false
        }
    }

    /// Stop tracking task execution and record energy
    pub fn stop_task(&mut self, task_id: TaskId, timestamp: u64) -> Option<Energy> {
        // Find the task
        let profile = self
            .tasks
            .iter_mut()
            .flatten()
            .find(|p| p.task_id == Some(task_id))?;

        let start = profile.start_timestamp.take()?;
        let duration = timestamp.saturating_sub(start);
        let energy = self
            .power_model
            .estimate_cpu_energy(duration, self.cpu_frequency);

        profile.total_energy += energy;
        profile.execution_count += 1;
        profile.total_execution_time += duration;
        self.total_system_energy += energy;

        Some(energy)
    }

    /// Record power mode change
    pub fn record_mode_change(&mut self, mode: super::power::PowerMode, timestamp: u64) {
        self.mode_stats.start_mode(mode, timestamp);
    }

    /// Get scheduling hint for a task
    pub fn get_scheduling_hint(&self, task_id: TaskId) -> SchedulingHint {
        if let Some(profile) = self.get_task(task_id) {
            // Check if over budget
            if profile.is_over_budget() {
                if let Some(budget) = &profile.budget {
                    return match budget.on_exceeded {
                        BudgetAction::Throttle => SchedulingHint::Throttle { factor: 50 },
                        BudgetAction::Suspend => SchedulingHint::Suspend,
                        BudgetAction::Migrate => SchedulingHint::Migrate,
                        BudgetAction::LogOnly => SchedulingHint::FullSpeed,
                    };
                }
            }

            // Check if approaching budget (within 20%)
            if let Some(remaining) = profile.remaining_budget() {
                let avg_energy = profile.average_energy_per_execution();
                if avg_energy > Energy::ZERO && remaining < avg_energy {
                    return SchedulingHint::Throttle { factor: 75 };
                }
            }
        }

        // Check system budget
        if let Some(budget) = self.system_budget {
            if self.total_system_energy > budget {
                return SchedulingHint::Throttle { factor: 50 };
            }
        }

        SchedulingHint::FullSpeed
    }

    /// Get total system energy consumed
    pub fn total_system_energy(&self) -> Energy {
        self.total_system_energy
    }

    /// Get remaining system energy budget
    pub fn remaining_system_budget(&self) -> Option<Energy> {
        self.system_budget
            .map(|b| b.saturating_sub(self.total_system_energy))
    }

    /// Reset all energy statistics
    pub fn reset(&mut self) {
        self.tasks.fill(None);
        self.mode_stats = PowerModeStats::new();
        self.total_system_energy = Energy::ZERO;
    }

    /// Get summary of all tracked tasks
    pub fn task_summary(&self) -> TaskEnergySummary {
        let mut total_energy = Energy::ZERO;
        let mut task_count = 0;
        let mut over_budget_count = 0;

        for profile in self.tasks.iter().flatten() {
            total_energy += profile.total_energy;
            task_count += 1;
            if profile.is_over_budget() {
                over_budget_count += 1;
            }
        }

        TaskEnergySummary {
            total_energy,
            task_count,
            over_budget_count,
        }
    }
}

impl Default for EnergyProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of task energy consumption
#[derive(Debug, Clone)]
pub struct TaskEnergySummary {
    /// Total energy consumed by all tasks
    pub total_energy: Energy,
    /// Number of tracked tasks
    pub task_count: usize,
    /// Number of tasks over budget
    pub over_budget_count: usize,
}

// ============================================================================
// EnergyError
// ============================================================================

/// Errors arising from energy subsystem operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyError {
    /// No slot available to register a new domain
    DomainTableFull,
    /// The requested peripheral is not registered
    DomainNotFound,
    /// Budget window overflow or invalid parameters
    InvalidBudget,
}

// ============================================================================
// EnergyPolicy / EnergyMode / SleepRecommendation
// ============================================================================

/// Device-wide energy operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyMode {
    /// Maximize performance; energy is irrelevant
    Performance,
    /// Balance performance and energy (default)
    Balanced,
    /// Minimize energy; accept latency degradation
    PowerSave,
    /// Honor an explicit rolling-window energy budget
    BudgetEnforced {
        /// Maximum energy (millijoules) over the window
        total_mj: u64,
        /// Window length in milliseconds
        window_ms: u64,
    },
}

/// Recommendation for the next CPU/system sleep depth
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepRecommendation {
    /// Do not sleep; keep the CPU running
    StayAwake,
    /// WFI / C1 – very fast wakeup
    LightSleep,
    /// STOP / C6 – clocks off, RAM retained
    DeepSleep,
    /// STANDBY – lowest power, RAM may be lost
    Hibernate,
}

/// Global energy policy that all subsystems consult
#[derive(Debug, Clone, Copy)]
pub struct EnergyPolicy {
    /// Operating mode
    pub mode: EnergyMode,
    /// Hard cap on instantaneous total power (milliwatts)
    pub max_power_mw: u32,
    /// Idle time (ms) before suggesting deep sleep
    pub idle_threshold_ms: u64,
    /// Acceptable wakeup latency budget (microseconds)
    pub wakeup_latency_us: u64,
}

impl EnergyPolicy {
    /// Full-speed policy — no energy constraints applied
    pub fn performance() -> Self {
        Self {
            mode: EnergyMode::Performance,
            max_power_mw: u32::MAX,
            idle_threshold_ms: u64::MAX,
            wakeup_latency_us: 0,
        }
    }

    /// Balanced policy — mild power constraints, fast wakeup
    pub fn balanced() -> Self {
        Self {
            mode: EnergyMode::Balanced,
            max_power_mw: 500,
            idle_threshold_ms: 50,
            wakeup_latency_us: 500,
        }
    }

    /// Aggressive power save — deep sleep as early as possible
    pub fn power_save() -> Self {
        Self {
            mode: EnergyMode::PowerSave,
            max_power_mw: 100,
            idle_threshold_ms: 5,
            wakeup_latency_us: 10_000,
        }
    }

    /// Hard budget over a rolling window
    pub fn budget_enforced(total_mj: u64, window_ms: u64) -> Self {
        Self {
            mode: EnergyMode::BudgetEnforced {
                total_mj,
                window_ms,
            },
            max_power_mw: ((total_mj * 1_000) / window_ms.max(1)) as u32,
            idle_threshold_ms: 20,
            wakeup_latency_us: 2_000,
        }
    }

    /// Recommend a sleep depth given how long the system has been idle and
    /// when the next task deadline arrives.
    ///
    /// The recommendation becomes deeper as idle time grows relative to the
    /// idle threshold and the deadline distance exceeds the acceptable wakeup
    /// latency.
    pub fn recommend_sleep(&self, idle_ms: u64, next_deadline_ms: u64) -> SleepRecommendation {
        match self.mode {
            EnergyMode::Performance => SleepRecommendation::StayAwake,

            EnergyMode::Balanced | EnergyMode::BudgetEnforced { .. } => {
                if idle_ms == 0 {
                    return SleepRecommendation::StayAwake;
                }
                let wakeup_ms = self.wakeup_latency_us / 1_000;
                if next_deadline_ms <= wakeup_ms + 1 {
                    SleepRecommendation::StayAwake
                } else if idle_ms < self.idle_threshold_ms / 2 {
                    SleepRecommendation::LightSleep
                } else if idle_ms < self.idle_threshold_ms {
                    SleepRecommendation::DeepSleep
                } else {
                    SleepRecommendation::Hibernate
                }
            }

            EnergyMode::PowerSave => {
                if idle_ms == 0 {
                    return SleepRecommendation::LightSleep;
                }
                let wakeup_ms = self.wakeup_latency_us / 1_000;
                if next_deadline_ms <= wakeup_ms + 1 {
                    SleepRecommendation::LightSleep
                } else if idle_ms < self.idle_threshold_ms {
                    SleepRecommendation::DeepSleep
                } else {
                    SleepRecommendation::Hibernate
                }
            }
        }
    }
}

// ============================================================================
// PowerDomain / PeripheralId / DomainState / PowerDomainTracker / DomainReport
// ============================================================================

/// Identifier for a trackable peripheral power domain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeripheralId {
    Cpu,
    Ram,
    Flash,
    Uart0,
    Uart1,
    Spi0,
    Spi1,
    I2c0,
    I2c1,
    Gpio,
    Timer0,
    Timer1,
    Radio,
    Adc,
    /// User-defined peripheral (index 0–254)
    Custom(u8),
}

impl PeripheralId {
    fn index(&self) -> usize {
        match self {
            PeripheralId::Cpu => 0,
            PeripheralId::Ram => 1,
            PeripheralId::Flash => 2,
            PeripheralId::Uart0 => 3,
            PeripheralId::Uart1 => 4,
            PeripheralId::Spi0 => 5,
            PeripheralId::Spi1 => 6,
            PeripheralId::I2c0 => 7,
            PeripheralId::I2c1 => 8,
            PeripheralId::Gpio => 9,
            PeripheralId::Timer0 => 10,
            PeripheralId::Timer1 => 11,
            PeripheralId::Radio => 12,
            PeripheralId::Adc => 13,
            PeripheralId::Custom(n) => 14usize.saturating_add(*n as usize),
        }
    }
}

/// Operating state of a power domain
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainState {
    /// Peripheral is fully active
    On,
    /// Clock-gated (register state preserved, no computation)
    Idle,
    /// Fully powered off
    Off,
}

/// Per-peripheral power domain with energy accounting
#[derive(Debug, Clone, Copy)]
pub struct PowerDomain {
    /// Which peripheral this domain represents
    pub peripheral: PeripheralId,
    /// Power draw when fully active (milliwatts)
    pub power_on_mw: u16,
    /// Power draw when clock-gated (milliwatts)
    pub power_idle_mw: u16,
    /// Power draw when fully off (milliwatts — e.g. leakage)
    pub power_off_mw: u16,
    /// Current operating state
    pub state: DomainState,
    /// Cumulative energy consumed in all states
    pub energy_consumed: Energy,
    /// Timestamp of last state transition (microseconds)
    pub last_state_change_us: u64,
}

impl PowerDomain {
    /// Construct a new power domain starting in the Off state
    pub fn new(
        peripheral: PeripheralId,
        power_on_mw: u16,
        power_idle_mw: u16,
        power_off_mw: u16,
        start_us: u64,
    ) -> Self {
        Self {
            peripheral,
            power_on_mw,
            power_idle_mw,
            power_off_mw,
            state: DomainState::Off,
            energy_consumed: Energy::ZERO,
            last_state_change_us: start_us,
        }
    }

    /// Instantaneous power draw based on current state (milliwatts)
    pub fn current_power_mw(&self) -> u32 {
        match self.state {
            DomainState::On => self.power_on_mw as u32,
            DomainState::Idle => self.power_idle_mw as u32,
            DomainState::Off => self.power_off_mw as u32,
        }
    }

    /// Accrue energy for time spent in current state, then transition
    pub fn transition(&mut self, new_state: DomainState, now_us: u64) {
        let elapsed_us = now_us.saturating_sub(self.last_state_change_us);
        let power_mw = self.current_power_mw() as u64;
        // E[µJ] = P[mW] * t[µs] / 1000
        let energy_uj = power_mw.saturating_mul(elapsed_us) / 1_000;
        self.energy_consumed = self
            .energy_consumed
            .saturating_add(Energy::microjoules(energy_uj));
        self.state = new_state;
        self.last_state_change_us = now_us;
    }
}

/// Maximum number of simultaneously tracked power domains
const MAX_POWER_DOMAINS: usize = 16;

/// Summary report across all registered domains
#[derive(Debug, Clone, Copy)]
pub struct DomainReport {
    /// Number of registered domains
    pub domain_count: usize,
    /// Total instantaneous power draw (milliwatts)
    pub total_power_mw: u32,
    /// Total energy consumed across all domains (microjoules)
    pub total_energy_uj: u64,
    /// Number of domains currently active
    pub active_domains: usize,
}

/// Tracker for an ensemble of power domains
pub struct PowerDomainTracker {
    domains: [Option<PowerDomain>; MAX_POWER_DOMAINS],
    total_power_mw: core::sync::atomic::AtomicU32,
    tracking_start_us: u64,
}

impl PowerDomainTracker {
    /// Create a new tracker with the given wall-clock origin
    pub fn new(start_us: u64) -> Self {
        Self {
            domains: [const { None }; MAX_POWER_DOMAINS],
            total_power_mw: core::sync::atomic::AtomicU32::new(0),
            tracking_start_us: start_us,
        }
    }

    /// Register a new power domain.  Returns an error if the table is full or
    /// the domain's index slot is already occupied.
    pub fn register(&mut self, domain: PowerDomain) -> Result<(), EnergyError> {
        let idx = domain.peripheral.index() % MAX_POWER_DOMAINS;
        if self.domains[idx].is_some() {
            return Err(EnergyError::DomainTableFull);
        }
        let pwr = domain.current_power_mw();
        self.domains[idx] = Some(domain);
        self.recompute_total_power();
        let _ = pwr;
        Ok(())
    }

    /// Transition a peripheral to a new state, accruing energy for the elapsed
    /// time in the previous state.
    pub fn transition(&mut self, peripheral: PeripheralId, new_state: DomainState, now_us: u64) {
        let idx = peripheral.index() % MAX_POWER_DOMAINS;
        if let Some(ref mut domain) = self.domains[idx] {
            domain.transition(new_state, now_us);
            self.recompute_total_power();
        }
    }

    /// Instantaneous total power draw from all registered domains (milliwatts)
    pub fn total_power_mw(&self) -> u32 {
        self.total_power_mw
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Accumulated energy across all domains since tracking started (microjoules)
    pub fn total_energy_uj(&self) -> u64 {
        self.domains
            .iter()
            .flatten()
            .map(|d| d.energy_consumed.as_microjoules())
            .fold(0u64, |a, b| a.saturating_add(b))
    }

    /// Energy consumed by a specific peripheral, or None if not registered
    pub fn peripheral_energy(&self, peripheral: PeripheralId) -> Option<Energy> {
        let idx = peripheral.index() % MAX_POWER_DOMAINS;
        self.domains[idx].as_ref().map(|d| d.energy_consumed)
    }

    /// Produce a summary report
    pub fn report(&self) -> DomainReport {
        let mut domain_count = 0usize;
        let mut active_domains = 0usize;
        for d in self.domains.iter().flatten() {
            domain_count += 1;
            if d.state == DomainState::On {
                active_domains += 1;
            }
        }
        DomainReport {
            domain_count,
            total_power_mw: self.total_power_mw(),
            total_energy_uj: self.total_energy_uj(),
            active_domains,
        }
    }

    /// Tracking origin timestamp
    pub fn tracking_start_us(&self) -> u64 {
        self.tracking_start_us
    }

    fn recompute_total_power(&self) {
        let total: u32 = self
            .domains
            .iter()
            .flatten()
            .map(|d| d.current_power_mw())
            .fold(0u32, |a, b| a.saturating_add(b));
        self.total_power_mw
            .store(total, core::sync::atomic::Ordering::Relaxed);
    }
}

// ============================================================================
// PowerManager — energy-policy-aware facade over AdvancedPowerManager
// ============================================================================

/// Power state exposed to the energy-aware scheduler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Full CPU activity
    Active,
    /// WFI-class light sleep
    LightSleep,
    /// STOP-class deep sleep
    DeepSleep,
    /// STANDBY-class hibernate
    Hibernate,
}

/// Thin power manager that integrates an [`EnergyPolicy`] with a power-state
/// machine.  Deliberately separate from the heavy `AdvancedPowerManager` in
/// `power.rs` so energy_scheduler can own it without alloc.
#[derive(Debug)]
pub struct PowerManager {
    policy: EnergyPolicy,
    state: PowerState,
}

impl PowerManager {
    /// Create a new manager with an explicit energy policy
    pub fn with_energy_policy(policy: EnergyPolicy) -> Self {
        Self {
            policy,
            state: PowerState::Active,
        }
    }

    /// Map a scheduling hint's sleep recommendation to a concrete `PowerState`
    pub fn suggest_next_state(
        &self,
        hint: &crate::energy_scheduler::EnergySchedulingHint,
    ) -> PowerState {
        match hint.sleep_recommendation {
            SleepRecommendation::StayAwake => PowerState::Active,
            SleepRecommendation::LightSleep => PowerState::LightSleep,
            SleepRecommendation::DeepSleep => PowerState::DeepSleep,
            SleepRecommendation::Hibernate => PowerState::Hibernate,
        }
    }

    /// Current power state
    pub fn state(&self) -> PowerState {
        self.state
    }

    /// Apply a new power state
    pub fn set_state(&mut self, state: PowerState) {
        self.state = state;
    }

    /// Expose the active policy
    pub fn policy(&self) -> &EnergyPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::PowerMode;

    #[test]
    fn test_energy_units() {
        let e1 = Energy::microjoules(1000);
        assert_eq!(e1.as_microjoules(), 1000);
        assert_eq!(e1.as_millijoules(), 1);

        let e2 = Energy::millijoules(5);
        assert_eq!(e2.as_microjoules(), 5000);
        assert_eq!(e2.as_millijoules(), 5);

        let e3 = Energy::joules(1);
        assert_eq!(e3.as_microjoules(), 1_000_000);
        assert_eq!(e3.as_millijoules(), 1000);
    }

    #[test]
    fn test_energy_arithmetic() {
        let e1 = Energy::millijoules(10);
        let e2 = Energy::millijoules(5);

        assert_eq!((e1 + e2).as_millijoules(), 15);
        assert_eq!((e1 - e2).as_millijoules(), 5);
        assert_eq!(e1.saturating_sub(Energy::millijoules(20)), Energy::ZERO);
    }

    #[test]
    fn test_power_model() {
        let model = PowerModel::default();

        // Test CPU energy estimation
        // 1 second at 80 MHz: Power = 1000 + (50 * 80) = 5000 µW = 5 mW
        // Energy = 5000 µW × 1000000 µs / 1000000 = 5000 µJ = 5 mJ
        let energy = model.estimate_cpu_energy(1_000_000, 80);
        assert_eq!(energy.as_millijoules(), 5);

        // Test radio TX energy
        // 100ms at 30 mW: Energy = 30000 µW × 100000 µs / 1000000 = 3000 µJ = 3 mJ
        let tx_energy = model.estimate_radio_tx_energy(100_000);
        assert_eq!(tx_energy.as_millijoules(), 3);
    }

    #[test]
    fn test_power_model_profiles() {
        let low_power = PowerModel::low_power();
        let high_perf = PowerModel::high_performance();

        // Low power should use less energy
        let low_energy = low_power.estimate_cpu_energy(1_000_000, 48);
        let high_energy = high_perf.estimate_cpu_energy(1_000_000, 400);

        assert!(low_energy < high_energy);
    }

    #[test]
    fn test_task_energy_profile() {
        let task_id = TaskId::new(1);
        let mut profile = TaskEnergyProfile::new(task_id);

        assert_eq!(profile.task_id, Some(task_id));
        assert_eq!(profile.total_energy, Energy::ZERO);
        assert!(!profile.is_over_budget());

        // Set budget
        profile.set_budget(EnergyBudget::millijoules(100));
        assert!(profile.remaining_budget().is_some());
        assert_eq!(profile.remaining_budget().unwrap().as_millijoules(), 100);

        // Add energy consumption
        profile.total_energy = Energy::millijoules(50);
        profile.execution_count = 5;
        profile.total_execution_time = 50_000;

        assert!(!profile.is_over_budget());
        assert_eq!(profile.remaining_budget().unwrap().as_millijoules(), 50);
        assert_eq!(profile.average_energy_per_execution().as_millijoules(), 10);
        assert_eq!(profile.average_execution_time(), 10_000);

        // Exceed budget
        profile.total_energy = Energy::millijoules(150);
        assert!(profile.is_over_budget());
    }

    #[test]
    fn test_energy_profiler_basic() {
        let mut profiler = EnergyProfiler::new();
        let task_id = TaskId::new(1);

        // Start and stop task
        assert!(profiler.start_task(task_id, 0));
        let energy = profiler.stop_task(task_id, 1_000_000); // 1 second
        assert!(energy.is_some());

        // Check task profile
        let profile = profiler.get_task(task_id).unwrap();
        assert_eq!(profile.execution_count, 1);
        assert!(profile.total_energy > Energy::ZERO);
    }

    #[test]
    fn test_energy_profiler_budget() {
        let mut profiler = EnergyProfiler::new();
        let task_id = TaskId::new(1);

        // Set a small budget
        assert!(profiler.set_budget(task_id, EnergyBudget::millijoules(1)));

        // Run task multiple times to exceed budget
        for i in 0..10 {
            profiler.start_task(task_id, i * 1_000_000);
            profiler.stop_task(task_id, (i + 1) * 1_000_000);
        }

        // Should be over budget
        let profile = profiler.get_task(task_id).unwrap();
        assert!(profile.is_over_budget());

        // Scheduling hint should suggest throttling
        let hint = profiler.get_scheduling_hint(task_id);
        assert!(matches!(hint, SchedulingHint::Throttle { .. }));
    }

    #[test]
    fn test_power_mode_stats() {
        let mut stats = PowerModeStats::new();

        // Simulate mode transitions
        stats.start_mode(PowerMode::Normal, 0);
        stats.start_mode(PowerMode::LowPower, 1_000_000); // 1 second in Normal
        stats.start_mode(PowerMode::Sleep, 2_000_000); // 1 second in LowPower
        stats.start_mode(PowerMode::Normal, 5_000_000); // 3 seconds in Sleep

        assert_eq!(stats.normal_time, 1_000_000);
        assert_eq!(stats.low_power_time, 1_000_000);
        assert_eq!(stats.sleep_time, 3_000_000);
        assert_eq!(stats.total_time(), 5_000_000);

        let percentages = stats.mode_percentages();
        assert_eq!(percentages.normal, 20);
        assert_eq!(percentages.low_power, 20);
        assert_eq!(percentages.sleep, 60);
    }

    #[test]
    fn test_system_budget() {
        let mut profiler = EnergyProfiler::new();

        // Set system budget
        profiler.set_system_budget(Energy::millijoules(10));

        // Run tasks to consume energy
        for i in 0..100 {
            let task_id = TaskId::new(i % 5);
            profiler.start_task(task_id, i as u64 * 100_000);
            profiler.stop_task(task_id, (i + 1) as u64 * 100_000);
        }

        // Check remaining budget
        assert!(profiler.remaining_system_budget().is_some());
    }

    #[test]
    fn test_task_summary() {
        let mut profiler = EnergyProfiler::new();

        // Create multiple tasks
        for i in 0..5 {
            let task_id = TaskId::new(i);
            profiler.set_budget(task_id, EnergyBudget::millijoules(100));
            profiler.start_task(task_id, i as u64 * 1_000_000);
            profiler.stop_task(task_id, (i + 1) as u64 * 1_000_000);
        }

        let summary = profiler.task_summary();
        assert_eq!(summary.task_count, 5);
        assert!(summary.total_energy > Energy::ZERO);
    }

    #[test]
    fn test_scheduling_hints() {
        let mut profiler = EnergyProfiler::new();
        let task_id = TaskId::new(1);

        // Task without budget should get FullSpeed hint
        profiler.start_task(task_id, 0);
        profiler.stop_task(task_id, 1000);
        assert_eq!(
            profiler.get_scheduling_hint(task_id),
            SchedulingHint::FullSpeed
        );

        // Task with exceeded budget should get throttled
        let task_id2 = TaskId::new(2);
        profiler.set_budget(
            task_id2,
            EnergyBudget::microjoules(1).with_action(BudgetAction::Suspend),
        );
        for i in 0..100 {
            profiler.start_task(task_id2, i * 1_000_000);
            profiler.stop_task(task_id2, (i + 1) * 1_000_000);
        }
        assert_eq!(
            profiler.get_scheduling_hint(task_id2),
            SchedulingHint::Suspend
        );
    }

    #[test]
    fn test_profiler_reset() {
        let mut profiler = EnergyProfiler::new();
        let task_id = TaskId::new(1);

        profiler.start_task(task_id, 0);
        profiler.stop_task(task_id, 1_000_000);
        assert!(profiler.total_system_energy() > Energy::ZERO);

        profiler.reset();
        assert_eq!(profiler.total_system_energy(), Energy::ZERO);
        assert!(profiler.get_task(task_id).is_none());
    }

    #[test]
    fn test_custom_power_model() {
        let model = PowerModel::new(2000, 100, 2, 50_000, 25_000);
        let profiler = EnergyProfiler::with_power_model(model);

        assert_eq!(profiler.power_model().idle_power, 2000);
        assert_eq!(profiler.power_model().cpu_power_per_mhz, 100);
    }

    #[test]
    fn test_energy_budget_actions() {
        let throttle = EnergyBudget::millijoules(100);
        assert_eq!(throttle.on_exceeded, BudgetAction::Throttle);

        let suspend = EnergyBudget::millijoules(100).with_action(BudgetAction::Suspend);
        assert_eq!(suspend.on_exceeded, BudgetAction::Suspend);

        let migrate = EnergyBudget::millijoules(100).with_action(BudgetAction::Migrate);
        assert_eq!(migrate.on_exceeded, BudgetAction::Migrate);
    }

    #[test]
    fn test_idle_task() {
        let mut profiler = EnergyProfiler::new();

        profiler.start_task(TaskId::IDLE, 0);
        let energy = profiler.stop_task(TaskId::IDLE, 1_000_000);
        assert!(energy.is_some());

        let profile = profiler.get_task(TaskId::IDLE).unwrap();
        assert_eq!(profile.execution_count, 1);
    }

    // ------------------------------------------------------------------
    // EnergyPolicy tests
    // ------------------------------------------------------------------

    #[test]
    fn test_energy_policy_performance_always_awake() {
        let policy = EnergyPolicy::performance();
        // Performance mode never sleeps regardless of idle time
        assert_eq!(
            policy.recommend_sleep(10_000, 100_000),
            SleepRecommendation::StayAwake
        );
        assert_eq!(policy.recommend_sleep(0, 0), SleepRecommendation::StayAwake);
    }

    #[test]
    fn test_energy_policy_power_save_recommends_sleep() {
        let policy = EnergyPolicy::power_save();
        // Any non-zero idle time should at minimum get LightSleep under PowerSave
        let rec = policy.recommend_sleep(0, 100_000);
        assert_eq!(rec, SleepRecommendation::LightSleep);

        // Past threshold with ample deadline → Hibernate
        let rec = policy.recommend_sleep(policy.idle_threshold_ms + 1, 100_000);
        assert_eq!(rec, SleepRecommendation::Hibernate);
    }

    #[test]
    fn test_energy_policy_sleep_recommendation_deepens_with_idle_time() {
        let policy = EnergyPolicy::balanced();
        let long_deadline = 1_000_000;

        // No idle → stay awake
        assert_eq!(
            policy.recommend_sleep(0, long_deadline),
            SleepRecommendation::StayAwake
        );
        // Just past half threshold → DeepSleep (or beyond)
        let half = policy.idle_threshold_ms / 2;
        let rec_mid = policy.recommend_sleep(half + 1, long_deadline);
        assert!(matches!(
            rec_mid,
            SleepRecommendation::DeepSleep | SleepRecommendation::Hibernate
        ));
        // Past full threshold → Hibernate
        assert_eq!(
            policy.recommend_sleep(policy.idle_threshold_ms + 1, long_deadline),
            SleepRecommendation::Hibernate
        );
    }

    #[test]
    fn test_energy_policy_budget_enforced() {
        let policy = EnergyPolicy::budget_enforced(100, 1_000);
        assert!(matches!(policy.mode, EnergyMode::BudgetEnforced { .. }));
        // max_power_mw = 100_000 µJ / 1_000 ms = 100 mW
        assert_eq!(policy.max_power_mw, 100);
    }

    // ------------------------------------------------------------------
    // PowerDomainTracker tests
    // ------------------------------------------------------------------

    #[test]
    fn test_power_domain_tracker_register() {
        let mut tracker = PowerDomainTracker::new(0);
        let domain = PowerDomain::new(PeripheralId::Cpu, 200, 50, 5, 0);
        assert!(tracker.register(domain).is_ok());
        assert_eq!(tracker.report().domain_count, 1);
    }

    #[test]
    fn test_power_domain_tracker_transition_accounting() {
        let mut tracker = PowerDomainTracker::new(0);
        // CPU: 200 mW on, 50 mW idle, 5 mW off; starts Off
        let domain = PowerDomain::new(PeripheralId::Cpu, 200, 50, 5, 0);
        tracker.register(domain).unwrap();

        // Transition Off→On at t=0 (0 µs spent Off)
        tracker.transition(PeripheralId::Cpu, DomainState::On, 0);
        // Run On for 1 000 µs, then go Idle
        tracker.transition(PeripheralId::Cpu, DomainState::Idle, 1_000);

        // Energy = 200 mW * 1000 µs / 1000 = 200 µJ
        let e = tracker.peripheral_energy(PeripheralId::Cpu).unwrap();
        assert_eq!(e.as_microjoules(), 200);
    }

    #[test]
    fn test_power_domain_tracker_total_power() {
        let mut tracker = PowerDomainTracker::new(0);
        let cpu = PowerDomain::new(PeripheralId::Cpu, 200, 50, 5, 0);
        let uart = PowerDomain::new(PeripheralId::Uart0, 10, 2, 0, 0);
        tracker.register(cpu).unwrap();
        tracker.register(uart).unwrap();

        // Both domains start Off; power = leakage only
        assert_eq!(tracker.total_power_mw(), 5); // CPU off=5, UART off=0

        // Bring CPU On
        tracker.transition(PeripheralId::Cpu, DomainState::On, 100);
        assert_eq!(tracker.total_power_mw(), 200); // CPU on=200, UART off=0
    }

    #[test]
    fn test_power_domain_tracker_report() {
        let mut tracker = PowerDomainTracker::new(0);
        tracker
            .register(PowerDomain::new(PeripheralId::Radio, 80, 10, 1, 0))
            .unwrap();
        tracker.transition(PeripheralId::Radio, DomainState::On, 0);
        tracker.transition(PeripheralId::Radio, DomainState::Off, 500);

        let report = tracker.report();
        assert_eq!(report.domain_count, 1);
        assert_eq!(report.active_domains, 0); // now Off
                                              // Energy: 80 mW * 500 µs / 1000 = 40 µJ
        assert_eq!(report.total_energy_uj, 40);
    }
}
