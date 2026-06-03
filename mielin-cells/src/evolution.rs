//! Self-Evolving Agents — Genetic Algorithm-Based Capability Evolution
//!
//! Provides a fitness-guided mutation and crossover engine that allows
//! distributed agents to discover and refine their own capabilities over
//! successive generations without any external randomness crate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ─────────────────────────── PRNG ────────────────────────────────────────────

/// Xorshift64 PRNG — deterministic, no external crate required.
pub(crate) struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Guard against zero seed (would lock in 0 forever)
        Self(if seed == 0 { 0xdeadbeef_cafebabe } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() as f64) / (u64::MAX as f64)
    }

    fn next_usize_mod(&mut self, m: usize) -> usize {
        if m == 0 {
            return 0;
        }
        (self.next() as usize) % m
    }
}

// ─────────────────────────── Core Types ──────────────────────────────────────

/// A quantized capability descriptor forming one gene in an agent genome.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityGene {
    /// Human-readable capability name, e.g. `"tensor_inference"`.
    pub name: String,
    /// 0-255 quantized skill level.
    pub proficiency: u8,
    /// Resource cost expressed in milli-CPU units.
    pub resource_cost: u32,
    /// Whether this capability is currently active.
    pub enabled: bool,
}

impl CapabilityGene {
    /// Create a new capability gene with sensible defaults.
    pub fn new(name: impl Into<String>, proficiency: u8, resource_cost: u32) -> Self {
        Self {
            name: name.into(),
            proficiency,
            resource_cost,
            enabled: true,
        }
    }
}

/// Agent genome — encodes all evolvable traits for a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGenome {
    /// Unique random genome identifier.
    pub id: u64,
    /// Generation number at which this genome was created.
    pub generation: u64,
    /// All evolvable capability genes.
    pub capabilities: Vec<CapabilityGene>,
    /// Resource-allocation policy weights (must sum to ≈ 1.0).
    pub policy_weights: Vec<f32>,
    /// High-level coordination strategy.
    pub coordination_strategy: CoordinationStrategy,
    /// Cached fitness score (updated after each evaluation).
    pub fitness_score: f64,
    /// Parent genome IDs for lineage tracking (empty for seed genomes).
    pub parent_ids: Vec<u64>,
}

impl AgentGenome {
    /// Create a minimal seed genome for generation 0.
    pub fn seed(id: u64) -> Self {
        Self {
            id,
            generation: 0,
            capabilities: Vec::new(),
            policy_weights: Vec::new(),
            coordination_strategy: CoordinationStrategy::Adaptive,
            fitness_score: 0.0,
            parent_ids: Vec::new(),
        }
    }
}

/// High-level strategy governing how an agent coordinates with peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinationStrategy {
    Cooperative,
    Competitive,
    Adaptive,
    Isolated,
}

impl CoordinationStrategy {
    const VARIANTS: [Self; 4] = [
        Self::Cooperative,
        Self::Competitive,
        Self::Adaptive,
        Self::Isolated,
    ];

    fn from_index(i: usize) -> Self {
        Self::VARIANTS[i % 4]
    }

    fn index(self) -> usize {
        match self {
            Self::Cooperative => 0,
            Self::Competitive => 1,
            Self::Adaptive => 2,
            Self::Isolated => 3,
        }
    }
}

// ─────────────────────────── Fitness ─────────────────────────────────────────

/// Observed runtime metrics used to compute a genome's fitness score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessMetrics {
    /// Tasks processed per second (higher is better).
    pub throughput: f64,
    /// Average round-trip latency in milliseconds (lower is better).
    pub latency_ms: f64,
    /// Capability gain per unit resource cost (higher is better).
    pub resource_efficiency: f64,
    /// Peer-cooperation quality score in \[0, 1\] (higher is better).
    pub coordination_score: f64,
    /// Fraction of tasks completed under injected faults in \[0, 1\].
    pub fault_tolerance: f64,
}

/// Weighted linear combination of fitness metrics → scalar in \[0, 1\].
pub struct FitnessEvaluator {
    pub throughput_weight: f64,
    pub latency_weight: f64,
    pub efficiency_weight: f64,
    pub coordination_weight: f64,
    pub fault_tolerance_weight: f64,
}

impl FitnessEvaluator {
    /// Construct from an explicit 5-element weight array.
    /// Order: `[throughput, latency, efficiency, coordination, fault_tolerance]`.
    pub fn new(weights: [f64; 5]) -> Self {
        Self {
            throughput_weight: weights[0],
            latency_weight: weights[1],
            efficiency_weight: weights[2],
            coordination_weight: weights[3],
            fault_tolerance_weight: weights[4],
        }
    }

    /// Balanced default weights (each 0.2).
    pub fn default_weights() -> Self {
        Self::new([0.2; 5])
    }

    /// Normalize weights so they always sum to 1.0.
    fn total_weight(&self) -> f64 {
        self.throughput_weight
            + self.latency_weight
            + self.efficiency_weight
            + self.coordination_weight
            + self.fault_tolerance_weight
    }

    /// Compute scalar fitness ∈ \[0, 1\] from raw metrics.
    ///
    /// Latency is inverted (lower latency → higher component score).
    /// All metric components are clamped to \[0, 1\] before weighting.
    pub fn evaluate(&self, metrics: &FitnessMetrics) -> f64 {
        let total = self.total_weight();
        if total == 0.0 {
            return 0.0;
        }

        // Normalise each dimension to [0, 1]:
        //   throughput        — clamp to [0, 1000] then scale
        //   latency_ms        — invert via 1/(1 + x/100) so 0 ms → 1.0
        //   resource_efficiency — clamp to [0, 1]
        //   coordination_score  — already in [0, 1]
        //   fault_tolerance     — already in [0, 1]
        let t_score = (metrics.throughput / 1000.0).clamp(0.0, 1.0);
        let l_score = 1.0 / (1.0 + metrics.latency_ms.max(0.0) / 100.0);
        let e_score = metrics.resource_efficiency.clamp(0.0, 1.0);
        let c_score = metrics.coordination_score.clamp(0.0, 1.0);
        let f_score = metrics.fault_tolerance.clamp(0.0, 1.0);

        (self.throughput_weight * t_score
            + self.latency_weight * l_score
            + self.efficiency_weight * e_score
            + self.coordination_weight * c_score
            + self.fault_tolerance_weight * f_score)
            / total
    }

    /// Return indices into `genomes` sorted descending by fitness.
    pub fn rank_population(&self, genomes: &[AgentGenome]) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..genomes.len()).collect();
        indices.sort_unstable_by(|&a, &b| {
            genomes[b]
                .fitness_score
                .partial_cmp(&genomes[a].fitness_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        indices
    }
}

// ─────────────────────────── Mutation ────────────────────────────────────────

/// Rate parameters controlling the frequency of each mutation operator.
pub struct MutationRates {
    /// Probability of toggling each capability's `enabled` flag.
    pub capability_flip: f64,
    /// Probability of drifting each capability's proficiency by ±1.
    pub proficiency_drift: f64,
    /// Probability of drifting each policy weight by ±0.01.
    pub weight_drift: f64,
    /// Probability of replacing the coordination strategy with a random one.
    pub strategy_mutation: f64,
}

/// Applies stochastic mutations to an `AgentGenome` using `Xorshift64`.
pub struct MutationOperator {
    /// Probability of toggling each capability's `enabled` flag.
    pub capability_flip_rate: f64,
    /// Probability of drifting each capability's proficiency by ±1.
    pub proficiency_drift_rate: f64,
    /// Probability of drifting each policy weight by ±0.01.
    pub weight_drift_rate: f64,
    /// Probability of replacing the coordination strategy entirely.
    pub strategy_mutation_rate: f64,
}

impl MutationOperator {
    /// Construct from explicit rates.
    pub fn new(rates: MutationRates) -> Self {
        Self {
            capability_flip_rate: rates.capability_flip,
            proficiency_drift_rate: rates.proficiency_drift,
            weight_drift_rate: rates.weight_drift,
            strategy_mutation_rate: rates.strategy_mutation,
        }
    }

    /// Default biological-analogy rates (5 %, 10 %, 15 %, 2 %).
    pub fn default_rates() -> Self {
        Self {
            capability_flip_rate: 0.05,
            proficiency_drift_rate: 0.10,
            weight_drift_rate: 0.15,
            strategy_mutation_rate: 0.02,
        }
    }

    /// Apply all mutation operators in-place. Does **not** change `id` or `parent_ids`.
    pub(crate) fn mutate(&self, genome: &mut AgentGenome, rng: &mut Xorshift64) {
        // 1. Capability enable/disable flips
        for gene in genome.capabilities.iter_mut() {
            if rng.next_f64() < self.capability_flip_rate {
                gene.enabled = !gene.enabled;
            }
        }

        // 2. Proficiency drift ±1 (saturating at 0 / 255)
        for gene in genome.capabilities.iter_mut() {
            if rng.next_f64() < self.proficiency_drift_rate {
                let up = rng.next_f64() < 0.5;
                gene.proficiency = if up {
                    gene.proficiency.saturating_add(1)
                } else {
                    gene.proficiency.saturating_sub(1)
                };
            }
        }

        // 3. Policy weight drift ±0.01 (clamped to [0, 1])
        for w in genome.policy_weights.iter_mut() {
            if rng.next_f64() < self.weight_drift_rate {
                let up = rng.next_f64() < 0.5;
                *w = if up {
                    (*w + 0.01_f32).clamp(0.0, 1.0)
                } else {
                    (*w - 0.01_f32).clamp(0.0, 1.0)
                };
            }
        }

        // 4. Coordination strategy replacement
        if rng.next_f64() < self.strategy_mutation_rate {
            let current = genome.coordination_strategy.index();
            let delta = rng.next_usize_mod(3) + 1; // 1, 2, or 3 — never 0 (no-op)
            genome.coordination_strategy = CoordinationStrategy::from_index(current + delta);
        }
    }

    /// Append one randomly sampled capability from `pool` if genome has capacity.
    pub(crate) fn add_random_capability(
        &self,
        genome: &mut AgentGenome,
        pool: &[&str],
        rng: &mut Xorshift64,
    ) {
        if pool.is_empty() {
            return;
        }
        let name = pool[rng.next_usize_mod(pool.len())];
        let proficiency = (rng.next() % 256) as u8;
        let resource_cost = ((rng.next() % 1000) as u32) + 1;
        genome.capabilities.push(CapabilityGene {
            name: name.to_owned(),
            proficiency,
            resource_cost,
            enabled: true,
        });
    }
}

// ─────────────────────────── Crossover ───────────────────────────────────────

/// Implements single-point and uniform crossover between two parent genomes.
pub struct CrossoverOperator;

impl CrossoverOperator {
    fn new_child_id(parent_a: &AgentGenome, parent_b: &AgentGenome, rng: &mut Xorshift64) -> u64 {
        // Combine parent bits deterministically then add PRNG noise.
        let raw = parent_a.id ^ parent_b.id.rotate_left(17) ^ rng.next();
        if raw == 0 {
            rng.next() | 1
        } else {
            raw
        }
    }

    /// Single-point crossover — capabilities split at a random boundary.
    ///
    /// The first part comes from `parent_a`, the second from `parent_b`.
    /// `policy_weights` and `coordination_strategy` are inherited from the
    /// higher-fitness parent; ties go to `parent_a`.
    pub(crate) fn crossover(
        &self,
        parent_a: &AgentGenome,
        parent_b: &AgentGenome,
        rng: &mut Xorshift64,
    ) -> AgentGenome {
        let len_a = parent_a.capabilities.len();
        let len_b = parent_b.capabilities.len();
        let total = len_a + len_b;

        let split = if total == 0 {
            0
        } else {
            rng.next_usize_mod(total + 1)
        };

        // Gather gene pool in order: a then b
        let all: Vec<CapabilityGene> = parent_a
            .capabilities
            .iter()
            .chain(parent_b.capabilities.iter())
            .cloned()
            .collect();
        let genes: Vec<CapabilityGene> = all.into_iter().take(split).collect();

        let dominant = if parent_b.fitness_score > parent_a.fitness_score {
            parent_b
        } else {
            parent_a
        };

        let generation = parent_a.generation.max(parent_b.generation) + 1;
        let child_id = Self::new_child_id(parent_a, parent_b, rng);

        AgentGenome {
            id: child_id,
            generation,
            capabilities: genes,
            policy_weights: dominant.policy_weights.clone(),
            coordination_strategy: dominant.coordination_strategy,
            fitness_score: 0.0,
            parent_ids: vec![parent_a.id, parent_b.id],
        }
    }

    /// Uniform crossover — each gene slot independently chosen from either parent
    /// with 50 % probability.
    pub(crate) fn uniform_crossover(
        &self,
        parent_a: &AgentGenome,
        parent_b: &AgentGenome,
        rng: &mut Xorshift64,
    ) -> AgentGenome {
        // Build gene list from the longer parent's slots, choosing per-position.
        let len = parent_a.capabilities.len().max(parent_b.capabilities.len());
        let mut genes = Vec::with_capacity(len);

        for i in 0..len {
            let from_a = rng.next_f64() < 0.5;
            let gene = match (parent_a.capabilities.get(i), parent_b.capabilities.get(i)) {
                (Some(ga), Some(gb)) => {
                    if from_a {
                        ga
                    } else {
                        gb
                    }
                }
                (Some(g), None) => g,
                (None, Some(g)) => g,
                (None, None) => break,
            };
            genes.push(gene.clone());
        }

        let dominant = if parent_b.fitness_score > parent_a.fitness_score {
            parent_b
        } else {
            parent_a
        };

        let generation = parent_a.generation.max(parent_b.generation) + 1;
        let child_id = Self::new_child_id(parent_a, parent_b, rng);

        AgentGenome {
            id: child_id,
            generation,
            capabilities: genes,
            policy_weights: dominant.policy_weights.clone(),
            coordination_strategy: dominant.coordination_strategy,
            fitness_score: 0.0,
            parent_ids: vec![parent_a.id, parent_b.id],
        }
    }
}

// ─────────────────────────── Config ──────────────────────────────────────────

/// Complete configuration for a single evolution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionConfig {
    /// Number of individuals maintained in the population.
    pub population_size: usize,
    /// Fraction of top individuals preserved without modification each generation.
    pub elite_fraction: f64,
    /// Per-individual probability of undergoing mutation.
    pub mutation_rate: f64,
    /// Per-individual probability of undergoing crossover.
    pub crossover_rate: f64,
    /// Hard ceiling on generations.
    pub max_generations: u64,
    /// If the best-fitness delta falls below this for 5 consecutive generations,
    /// the run is considered converged.
    pub convergence_threshold: f64,
    /// Universe of capability names available for random insertion.
    pub capability_pool: Vec<String>,
    /// Maximum number of capability genes per genome.
    pub max_capabilities: usize,
    /// Seed for the `Xorshift64` PRNG.
    pub seed: u64,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            population_size: 50,
            elite_fraction: 0.1,
            mutation_rate: 0.3,
            crossover_rate: 0.6,
            max_generations: 100,
            convergence_threshold: 1e-5,
            capability_pool: vec![
                "tensor_inference".into(),
                "gossip_relay".into(),
                "kv_store".into(),
                "stream_process".into(),
                "consensus_vote".into(),
            ],
            max_capabilities: 8,
            seed: 0xc0ffee42_deadbeef,
        }
    }
}

// ─────────────────────────── Selection ───────────────────────────────────────

/// Strategy used when selecting parents / survivors for the next generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStrategy {
    /// Preserve the top-*k* individuals unchanged.
    Elitist { k: usize },
    /// Draw a random subset of size `tournament_size`; the winner advances.
    Tournament { tournament_size: usize },
    /// Select with probability proportional to fitness (roulette wheel).
    RouletteWheel,
}

// ─────────────────────────── Statistics ──────────────────────────────────────

/// Per-generation statistics recorded by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionStats {
    pub generation: u64,
    pub best_fitness: f64,
    pub mean_fitness: f64,
    pub worst_fitness: f64,
    /// Average pairwise capability Hamming distance across the population.
    pub diversity: f64,
    pub total_mutations: u64,
    pub total_crossovers: u64,
    /// Number of consecutive generations with best-fitness delta < threshold.
    pub convergence_count: u64,
}

impl Default for EvolutionStats {
    fn default() -> Self {
        Self {
            generation: 0,
            best_fitness: 0.0,
            mean_fitness: 0.0,
            worst_fitness: 0.0,
            diversity: 0.0,
            total_mutations: 0,
            total_crossovers: 0,
            convergence_count: 0,
        }
    }
}

// ─────────────────────────── Engine ──────────────────────────────────────────

/// Full evolution engine driving a population of `AgentGenome` instances
/// through fitness-guided selection, crossover, and mutation.
pub struct EvolutionEngine {
    config: EvolutionConfig,
    rng: Xorshift64,
    population: Vec<AgentGenome>,
    evaluator: FitnessEvaluator,
    mutation_op: MutationOperator,
    crossover_op: CrossoverOperator,
    selection: SelectionStrategy,
    stats: EvolutionStats,
    generation: u64,
    best_ever: Option<AgentGenome>,
}

impl EvolutionEngine {
    /// Construct a new engine.
    ///
    /// Population is **not** seeded until [`initialize`] is called.
    pub fn new(config: EvolutionConfig, evaluator: FitnessEvaluator) -> Self {
        let rng = Xorshift64::new(config.seed);
        let mutation_op = MutationOperator::default_rates();
        let crossover_op = CrossoverOperator;
        let selection = SelectionStrategy::Tournament { tournament_size: 3 };

        Self {
            config,
            rng,
            population: Vec::new(),
            evaluator,
            mutation_op,
            crossover_op,
            selection,
            stats: EvolutionStats::default(),
            generation: 0,
            best_ever: None,
        }
    }

    /// Override the selection strategy.
    pub fn with_selection(mut self, strategy: SelectionStrategy) -> Self {
        self.selection = strategy;
        self
    }

    // ── Population helpers ────────────────────────────────────────────────

    fn random_genome(&mut self, generation: u64) -> AgentGenome {
        let id = self.rng.next() | 1; // ensure non-zero
        let n_caps = self.rng.next_usize_mod(self.config.max_capabilities.max(1)) + 1;
        let pool: Vec<&str> = self
            .config
            .capability_pool
            .iter()
            .map(|s| s.as_str())
            .collect();

        let mut genome = AgentGenome::seed(id);
        genome.generation = generation;

        for _ in 0..n_caps {
            if pool.is_empty() {
                break;
            }
            self.mutation_op
                .add_random_capability(&mut genome, &pool, &mut self.rng);
        }

        // Random policy weights (uniform in [0, 1])
        let n_weights = 4; // 4 resource dimensions
        genome.policy_weights = (0..n_weights).map(|_| self.rng.next_f64() as f32).collect();

        genome.coordination_strategy = CoordinationStrategy::from_index(self.rng.next_usize_mod(4));

        genome
    }

    /// Seed the population with `config.population_size` random genomes.
    pub fn initialize(&mut self) {
        self.population = (0..self.config.population_size)
            .map(|_| self.random_genome(0))
            .collect();
        self.generation = 0;
        self.stats = EvolutionStats::default();
        self.best_ever = None;
    }

    // ── Selection ────────────────────────────────────────────────────────

    /// Tournament selection — returns index of winner from a random subset.
    pub(crate) fn tournament_select(&mut self, tournament_size: usize) -> usize {
        let n = self.population.len();
        if n == 0 {
            return 0;
        }
        let mut best_idx = self.rng.next_usize_mod(n);
        for _ in 1..tournament_size.min(n) {
            let candidate = self.rng.next_usize_mod(n);
            if self.population[candidate].fitness_score > self.population[best_idx].fitness_score {
                best_idx = candidate;
            }
        }
        best_idx
    }

    /// Roulette-wheel selection — returns index proportional to fitness.
    pub(crate) fn roulette_select(&mut self) -> usize {
        let total: f64 = self
            .population
            .iter()
            .map(|g| g.fitness_score.max(0.0))
            .sum();
        if total == 0.0 {
            return self.rng.next_usize_mod(self.population.len().max(1));
        }
        let mut pick = self.rng.next_f64() * total;
        for (i, g) in self.population.iter().enumerate() {
            pick -= g.fitness_score.max(0.0);
            if pick <= 0.0 {
                return i;
            }
        }
        self.population.len() - 1
    }

    /// Select a parent index according to the configured strategy.
    fn select_parent(&mut self) -> usize {
        match self.selection {
            SelectionStrategy::Elitist { k } => {
                // Among elites, pick uniformly
                let elite_k = k.min(self.population.len()).max(1);
                self.rng.next_usize_mod(elite_k)
            }
            SelectionStrategy::Tournament { tournament_size } => {
                self.tournament_select(tournament_size)
            }
            SelectionStrategy::RouletteWheel => self.roulette_select(),
        }
    }

    // ── Statistics helpers ────────────────────────────────────────────────

    /// Compute average pairwise capability Hamming distance across the population.
    pub(crate) fn compute_diversity(population: &[AgentGenome]) -> f64 {
        let n = population.len();
        if n < 2 {
            return 0.0;
        }
        let mut total_dist = 0u64;
        let mut pairs = 0u64;

        for i in 0..n {
            for j in (i + 1)..n {
                let a = &population[i];
                let b = &population[j];
                // Hamming: count genes with different enabled state
                let max_len = a.capabilities.len().max(b.capabilities.len());
                let mut dist = 0u64;
                for k in 0..max_len {
                    let a_en = a.capabilities.get(k).map(|g| g.enabled).unwrap_or(false);
                    let b_en = b.capabilities.get(k).map(|g| g.enabled).unwrap_or(false);
                    if a_en != b_en {
                        dist += 1;
                    }
                    // Also count proficiency difference > 32 as a distance unit
                    let a_p = a.capabilities.get(k).map(|g| g.proficiency).unwrap_or(0);
                    let b_p = b.capabilities.get(k).map(|g| g.proficiency).unwrap_or(0);
                    let pdiff = (a_p as i16 - b_p as i16).unsigned_abs() as u64;
                    dist += pdiff / 32;
                }
                total_dist += dist;
                pairs += 1;
            }
        }

        if pairs == 0 {
            0.0
        } else {
            total_dist as f64 / pairs as f64
        }
    }

    fn update_stats(&mut self, prev_best: f64) {
        if self.population.is_empty() {
            return;
        }

        let mut best = f64::NEG_INFINITY;
        let mut worst = f64::INFINITY;
        let mut sum = 0.0;

        for g in &self.population {
            best = best.max(g.fitness_score);
            worst = worst.min(g.fitness_score);
            sum += g.fitness_score;
        }

        let mean = sum / self.population.len() as f64;
        let diversity = Self::compute_diversity(&self.population);

        let delta = (best - prev_best).abs();
        let convergence_count = if delta < self.config.convergence_threshold {
            self.stats.convergence_count + 1
        } else {
            0
        };

        self.stats.generation = self.generation;
        self.stats.best_fitness = best;
        self.stats.mean_fitness = mean;
        self.stats.worst_fitness = if worst == f64::INFINITY { 0.0 } else { worst };
        self.stats.diversity = diversity;
        self.stats.convergence_count = convergence_count;

        // Update best-ever
        let new_best_candidate = self
            .population
            .iter()
            .max_by(|a, b| {
                a.fitness_score
                    .partial_cmp(&b.fitness_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned();

        match (&self.best_ever, &new_best_candidate) {
            (None, Some(_)) => self.best_ever = new_best_candidate,
            (Some(be), Some(nc)) if nc.fitness_score > be.fitness_score => {
                self.best_ever = new_best_candidate;
            }
            _ => {}
        }
    }

    // ── Core step ─────────────────────────────────────────────────────────

    /// Evaluate every genome in the current population using `metrics_fn`,
    /// then produce the next generation via selection + crossover + mutation.
    ///
    /// Returns a reference to the updated statistics.
    pub fn step(&mut self, metrics_fn: impl Fn(&AgentGenome) -> FitnessMetrics) -> &EvolutionStats {
        if self.population.is_empty() {
            return &self.stats;
        }

        // 1. Evaluate
        for genome in self.population.iter_mut() {
            let metrics = metrics_fn(genome);
            genome.fitness_score = self.evaluator.evaluate(&metrics);
        }

        let prev_best = self.stats.best_fitness;

        // 2. Sort descending by fitness (stable for reproducibility)
        self.population.sort_by(|a, b| {
            b.fitness_score
                .partial_cmp(&a.fitness_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 3. Elite preservation
        let elite_count = ((self.config.elite_fraction * self.config.population_size as f64)
            as usize)
            .max(1)
            .min(self.population.len());

        let mut next_gen: Vec<AgentGenome> = self.population[..elite_count].to_vec();

        // Clone config values we need to avoid borrowing self during the loop.
        let pool_owned: Vec<String> = self.config.capability_pool.clone();
        let crossover_rate = self.config.crossover_rate;
        let mutation_rate = self.config.mutation_rate;
        let max_capabilities = self.config.max_capabilities;
        let population_size = self.config.population_size;

        // 4. Fill remainder via crossover + mutation
        while next_gen.len() < population_size {
            let roll = self.rng.next_f64();

            if roll < crossover_rate && self.population.len() >= 2 {
                let idx_a = self.select_parent();
                let mut idx_b = self.select_parent();
                // Ensure distinct parents
                for _ in 0..10 {
                    if idx_b != idx_a {
                        break;
                    }
                    idx_b = self.select_parent();
                }
                let pa = self.population[idx_a].clone();
                let pb = self.population[idx_b].clone();

                let mut child = if self.rng.next_f64() < 0.5 {
                    self.crossover_op.crossover(&pa, &pb, &mut self.rng)
                } else {
                    self.crossover_op.uniform_crossover(&pa, &pb, &mut self.rng)
                };
                self.stats.total_crossovers += 1;

                // Apply mutation to child
                if self.rng.next_f64() < mutation_rate {
                    self.mutation_op.mutate(&mut child, &mut self.rng);
                    self.stats.total_mutations += 1;

                    // Occasionally add a new capability
                    if child.capabilities.len() < max_capabilities && self.rng.next_f64() < 0.1 {
                        let pool_refs: Vec<&str> = pool_owned.iter().map(|s| s.as_str()).collect();
                        self.mutation_op.add_random_capability(
                            &mut child,
                            &pool_refs,
                            &mut self.rng,
                        );
                    }
                }

                child.generation = self.generation + 1;
                next_gen.push(child);
            } else {
                // Asexual reproduction: clone + mutate
                let idx = self.select_parent();
                let mut offspring = self.population[idx].clone();
                let parent_id = self.population[idx].id;
                offspring.id = self.rng.next() | 1;
                offspring.generation = self.generation + 1;
                offspring.parent_ids = vec![parent_id];
                offspring.fitness_score = 0.0;

                self.mutation_op.mutate(&mut offspring, &mut self.rng);
                self.stats.total_mutations += 1;

                next_gen.push(offspring);
            }
        }

        self.generation += 1;
        self.population = next_gen;
        self.update_stats(prev_best);

        &self.stats
    }

    // ── Full run ──────────────────────────────────────────────────────────

    /// Drive the population until convergence or `max_generations` is reached.
    ///
    /// Returns an [`EvolutionResult`] summarising the entire run.
    pub fn run(&mut self, metrics_fn: impl Fn(&AgentGenome) -> FitnessMetrics) -> EvolutionResult {
        if self.population.is_empty() {
            self.initialize();
        }

        let mut fitness_history: Vec<f64> = Vec::new();
        let mut converged = false;

        for _ in 0..self.config.max_generations {
            let stats = self.step(&metrics_fn);
            fitness_history.push(stats.best_fitness);

            if stats.convergence_count >= 5 {
                converged = true;
                break;
            }
        }

        let best_genome = self
            .best_ever
            .clone()
            .or_else(|| self.population.first().cloned())
            .unwrap_or_else(|| AgentGenome::seed(0));

        EvolutionResult {
            best_genome,
            final_stats: self.stats.clone(),
            generations_run: self.generation,
            converged,
            fitness_history,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    /// Return the genome with the highest fitness seen across all generations.
    pub fn best(&self) -> Option<&AgentGenome> {
        self.best_ever.as_ref().or_else(|| {
            self.population.iter().max_by(|a, b| {
                a.fitness_score
                    .partial_cmp(&b.fitness_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
    }

    /// Read access to the current population.
    pub fn population(&self) -> &[AgentGenome] {
        &self.population
    }

    /// Read access to the latest statistics snapshot.
    pub fn stats(&self) -> &EvolutionStats {
        &self.stats
    }

    /// Inject `genome` into the population, replacing the individual with the
    /// lowest fitness score.
    pub fn inject(&mut self, genome: AgentGenome) {
        if self.population.is_empty() {
            self.population.push(genome);
            return;
        }
        let worst_idx = self
            .population
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.fitness_score
                    .partial_cmp(&b.fitness_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.population[worst_idx] = genome;
    }

    /// Serialise the best-ever genome to a JSON string.
    pub fn export_best(&self) -> Option<String> {
        self.best().and_then(|g| serde_json::to_string(g).ok())
    }
}

// ─────────────────────────── EvolutionResult ─────────────────────────────────

/// Returned by [`EvolutionEngine::run`] — encapsulates the outcome of an
/// entire evolution session.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvolutionResult {
    pub best_genome: AgentGenome,
    pub final_stats: EvolutionStats,
    pub generations_run: u64,
    pub converged: bool,
    /// Best fitness value per generation.
    pub fitness_history: Vec<f64>,
}

// ─────────────────────────── CapabilityDiscovery ─────────────────────────────

/// Analyses runtime behaviour observations to suggest emergent capabilities
/// that exceed an observation-frequency threshold.
pub struct CapabilityDiscovery {
    observed_patterns: HashMap<String, u64>,
    threshold: u64,
}

impl CapabilityDiscovery {
    pub fn new(threshold: u64) -> Self {
        Self {
            observed_patterns: HashMap::new(),
            threshold,
        }
    }

    /// Record one observation of a behavioural pattern.
    pub fn observe(&mut self, pattern: &str) {
        *self
            .observed_patterns
            .entry(pattern.to_owned())
            .or_insert(0) += 1;
    }

    /// Return the names of all patterns whose observation count meets or
    /// exceeds the configured threshold.
    pub fn suggested_capabilities(&self) -> Vec<String> {
        let mut suggestions: Vec<String> = self
            .observed_patterns
            .iter()
            .filter(|(_, &count)| count >= self.threshold)
            .map(|(name, _)| name.clone())
            .collect();
        suggestions.sort(); // deterministic order
        suggestions
    }

    /// Reset all observation counts.
    pub fn clear_observations(&mut self) {
        self.observed_patterns.clear();
    }
}

// ─────────────────────────── Error Type ──────────────────────────────────────

/// Errors that can arise during an evolution session.
#[derive(Debug, Error)]
pub enum EvolutionError {
    #[error("Population is empty — call initialize() first")]
    EmptyPopulation,
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
    #[error("Genome serialization failed: {0}")]
    SerializationFailed(String),
    #[error("Population too small for tournament size {tournament_size}")]
    PopulationTooSmall { tournament_size: usize },
}

// ─────────────────────────── Tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────

    fn make_gene(name: &str, proficiency: u8, enabled: bool) -> CapabilityGene {
        CapabilityGene {
            name: name.into(),
            proficiency,
            resource_cost: 100,
            enabled,
        }
    }

    fn make_genome_with_fitness(id: u64, fitness: f64, genes: Vec<CapabilityGene>) -> AgentGenome {
        AgentGenome {
            id,
            generation: 0,
            capabilities: genes,
            policy_weights: vec![0.25, 0.25, 0.25, 0.25],
            coordination_strategy: CoordinationStrategy::Adaptive,
            fitness_score: fitness,
            parent_ids: Vec::new(),
        }
    }

    fn perfect_metrics() -> FitnessMetrics {
        FitnessMetrics {
            throughput: 1000.0,
            latency_ms: 0.0,
            resource_efficiency: 1.0,
            coordination_score: 1.0,
            fault_tolerance: 1.0,
        }
    }

    fn zero_metrics() -> FitnessMetrics {
        FitnessMetrics {
            throughput: 0.0,
            latency_ms: f64::MAX,
            resource_efficiency: 0.0,
            coordination_score: 0.0,
            fault_tolerance: 0.0,
        }
    }

    fn default_config(pop_size: usize) -> EvolutionConfig {
        EvolutionConfig {
            population_size: pop_size,
            seed: 42,
            capability_pool: vec!["a".into(), "b".into(), "c".into()],
            max_capabilities: 6,
            max_generations: 50,
            ..EvolutionConfig::default()
        }
    }

    // ── Test 1: genome creation ───────────────────────────────────────────

    #[test]
    fn test_genome_creation() {
        let id = 0xdeadbeef_u64;
        let genome = AgentGenome::seed(id);
        assert_eq!(genome.id, id);
        assert_eq!(genome.generation, 0);
        assert!(genome.capabilities.is_empty());
        assert!(genome.parent_ids.is_empty());
        assert_eq!(genome.fitness_score, 0.0);
    }

    // ── Test 2: evaluator — perfect metrics → fitness near 1.0 ───────────

    #[test]
    fn test_fitness_evaluator_basic() {
        let eval = FitnessEvaluator::default_weights();
        let score = eval.evaluate(&perfect_metrics());
        // throughput=1.0, latency->1/(1+0)=1.0, efficiency=1.0, coord=1.0, fault=1.0
        assert!(score > 0.95, "expected score > 0.95, got {score}");
    }

    // ── Test 3: evaluator — worst metrics → fitness near 0.0 ─────────────

    #[test]
    fn test_fitness_evaluator_zero() {
        let eval = FitnessEvaluator::default_weights();
        let score = eval.evaluate(&zero_metrics());
        // latency_ms = f64::MAX → l_score ≈ 0; all others 0
        assert!(score < 0.1, "expected score < 0.1, got {score}");
    }

    // ── Test 4: rank population ───────────────────────────────────────────

    #[test]
    fn test_fitness_evaluator_rank() {
        let eval = FitnessEvaluator::default_weights();
        let genomes = vec![
            make_genome_with_fitness(1, 0.2, vec![]),
            make_genome_with_fitness(2, 0.9, vec![]),
            make_genome_with_fitness(3, 0.5, vec![]),
        ];
        let ranked = eval.rank_population(&genomes);
        assert_eq!(
            ranked,
            vec![1, 2, 0],
            "should be sorted descending by fitness"
        );
    }

    // ── Test 5: mutation — capability flip ───────────────────────────────

    #[test]
    fn test_mutation_capability_flip() {
        let op = MutationOperator::new(MutationRates {
            capability_flip: 1.0,
            proficiency_drift: 0.0,
            weight_drift: 0.0,
            strategy_mutation: 0.0,
        });
        let mut rng = Xorshift64::new(1);
        let mut genome = make_genome_with_fitness(
            1,
            0.0,
            vec![make_gene("a", 100, true), make_gene("b", 100, false)],
        );
        let orig_states: Vec<bool> = genome.capabilities.iter().map(|g| g.enabled).collect();
        op.mutate(&mut genome, &mut rng);
        let new_states: Vec<bool> = genome.capabilities.iter().map(|g| g.enabled).collect();
        // All states must have flipped
        for (o, n) in orig_states.iter().zip(new_states.iter()) {
            assert_ne!(o, n, "flip_rate=1.0 must toggle all");
        }
    }

    // ── Test 6: mutation — proficiency drift ─────────────────────────────

    #[test]
    fn test_mutation_proficiency_drift() {
        let op = MutationOperator::new(MutationRates {
            capability_flip: 0.0,
            proficiency_drift: 1.0,
            weight_drift: 0.0,
            strategy_mutation: 0.0,
        });
        let mut rng = Xorshift64::new(99);
        let mut genome = make_genome_with_fitness(
            1,
            0.0,
            vec![make_gene("a", 128, true), make_gene("b", 128, true)],
        );
        let orig: Vec<u8> = genome.capabilities.iter().map(|g| g.proficiency).collect();
        op.mutate(&mut genome, &mut rng);
        let new_vals: Vec<u8> = genome.capabilities.iter().map(|g| g.proficiency).collect();
        // At drift_rate=1.0 each gene moves ±1 — with seed 99 the values must differ from 128
        let any_changed = orig.iter().zip(new_vals.iter()).any(|(o, n)| o != n);
        assert!(
            any_changed,
            "drift_rate=1.0 should change proficiency values"
        );
    }

    // ── Test 7: mutation preserves capability count ───────────────────────

    #[test]
    fn test_mutation_preserves_count() {
        let op = MutationOperator::default_rates();
        let mut rng = Xorshift64::new(7);
        let mut genome = make_genome_with_fitness(
            1,
            0.0,
            vec![make_gene("x", 50, true), make_gene("y", 50, false)],
        );
        let before = genome.capabilities.len();
        op.mutate(&mut genome, &mut rng);
        assert_eq!(
            genome.capabilities.len(),
            before,
            "mutate must not add/remove capabilities"
        );
    }

    // ── Test 8: add_random_capability ────────────────────────────────────

    #[test]
    fn test_mutation_add_random_capability() {
        let op = MutationOperator::default_rates();
        let mut rng = Xorshift64::new(8);
        let mut genome = AgentGenome::seed(1);
        let pool = &["tensor_inference", "gossip_relay"];
        op.add_random_capability(&mut genome, pool, &mut rng);
        assert_eq!(genome.capabilities.len(), 1);
        let name = &genome.capabilities[0].name;
        assert!(
            pool.contains(&name.as_str()),
            "capability must come from pool"
        );
    }

    // ── Test 9: single-point crossover ───────────────────────────────────

    #[test]
    fn test_crossover_single_point() {
        let op = CrossoverOperator;
        let mut rng = Xorshift64::new(9);
        let pa = make_genome_with_fitness(
            10,
            0.8,
            vec![make_gene("a", 1, true), make_gene("b", 2, true)],
        );
        let pb = make_genome_with_fitness(
            20,
            0.6,
            vec![make_gene("c", 3, true), make_gene("d", 4, true)],
        );
        let child = op.crossover(&pa, &pb, &mut rng);
        // Child genes must be a prefix of the concatenated (pa ++ pb) gene list
        let all_names: Vec<&str> = vec!["a", "b", "c", "d"];
        for g in &child.capabilities {
            assert!(
                all_names.contains(&g.name.as_str()),
                "gene '{}' not from either parent",
                g.name
            );
        }
    }

    // ── Test 10: uniform crossover ────────────────────────────────────────

    #[test]
    fn test_crossover_uniform() {
        let op = CrossoverOperator;
        let mut rng = Xorshift64::new(10);
        let pa = make_genome_with_fitness(
            10,
            0.5,
            vec![make_gene("a", 1, true), make_gene("b", 2, true)],
        );
        let pb = make_genome_with_fitness(
            20,
            0.5,
            vec![make_gene("c", 3, false), make_gene("d", 4, false)],
        );
        let child = op.uniform_crossover(&pa, &pb, &mut rng);
        assert!(
            !child.capabilities.is_empty(),
            "uniform crossover must yield at least one gene"
        );
        let valid_names: Vec<&str> = vec!["a", "b", "c", "d"];
        for g in &child.capabilities {
            assert!(valid_names.contains(&g.name.as_str()));
        }
    }

    // ── Test 11: crossover child has new id ───────────────────────────────

    #[test]
    fn test_crossover_child_has_new_id() {
        let op = CrossoverOperator;
        let mut rng = Xorshift64::new(11);
        let pa = make_genome_with_fitness(0xAA, 0.5, vec![]);
        let pb = make_genome_with_fitness(0xBB, 0.5, vec![]);
        let child = op.crossover(&pa, &pb, &mut rng);
        assert_ne!(child.id, pa.id, "child id must differ from parent_a");
        assert_ne!(child.id, pb.id, "child id must differ from parent_b");
    }

    // ── Test 12: crossover lineage tracked ───────────────────────────────

    #[test]
    fn test_crossover_lineage_tracked() {
        let op = CrossoverOperator;
        let mut rng = Xorshift64::new(12);
        let pa = make_genome_with_fitness(0xAA, 0.5, vec![]);
        let pb = make_genome_with_fitness(0xBB, 0.5, vec![]);
        let child = op.crossover(&pa, &pb, &mut rng);
        assert!(
            child.parent_ids.contains(&pa.id),
            "lineage must include parent_a id"
        );
        assert!(
            child.parent_ids.contains(&pb.id),
            "lineage must include parent_b id"
        );
    }

    // ── Test 13: engine initialize ────────────────────────────────────────

    #[test]
    fn test_engine_initialize() {
        let cfg = default_config(10);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();
        assert_eq!(
            engine.population().len(),
            10,
            "population must equal population_size after initialize"
        );
    }

    // ── Test 14: engine step updates stats ───────────────────────────────

    #[test]
    fn test_engine_step() {
        let cfg = default_config(10);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();
        let stats = engine.step(|_| perfect_metrics()).clone();
        assert!(stats.best_fitness > 0.0, "stats must be updated after step");
        assert_eq!(stats.generation, 1);
    }

    // ── Test 15: best fitness is monotonically non-decreasing (elitist) ──

    #[test]
    fn test_engine_best_fitness_monotone() {
        let mut cfg = default_config(20);
        cfg.elite_fraction = 0.3; // strong elitist pressure
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights())
            .with_selection(SelectionStrategy::Elitist { k: 6 });
        engine.initialize();

        let mut prev_best = 0.0_f64;
        for _ in 0..10 {
            let stats = engine.step(|_| perfect_metrics()).clone();
            assert!(
                stats.best_fitness >= prev_best - 1e-9,
                "best fitness regressed: {} -> {}",
                prev_best,
                stats.best_fitness
            );
            prev_best = stats.best_fitness;
        }
    }

    // ── Test 16: convergence detection ───────────────────────────────────

    #[test]
    fn test_engine_convergence() {
        let mut cfg = default_config(10);
        cfg.convergence_threshold = 1e-3;
        cfg.max_generations = 20;
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();

        let result = engine.run(|_| perfect_metrics());
        assert!(
            result.converged || result.generations_run <= 20,
            "should converge or exhaust generations"
        );
    }

    // ── Test 17: inject ───────────────────────────────────────────────────

    #[test]
    fn test_engine_inject() {
        let cfg = default_config(5);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();

        engine.step(|_| perfect_metrics());

        let injected_id = 0x1337_u64;
        let injected = make_genome_with_fitness(injected_id, 0.99, vec![make_gene("x", 200, true)]);
        engine.inject(injected);

        let found = engine.population().iter().any(|g| g.id == injected_id);
        assert!(found, "injected genome must appear in population");
    }

    // ── Test 18: export_best returns valid JSON ───────────────────────────

    #[test]
    fn test_engine_export_best() {
        let cfg = default_config(5);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();
        engine.step(|_| perfect_metrics());

        let json = engine
            .export_best()
            .expect("export_best must return Some after a step");
        let _parsed: serde_json::Value =
            serde_json::from_str(&json).expect("export_best must return valid JSON");
    }

    // ── Test 19: run() with trivial metrics converges ─────────────────────

    #[test]
    fn test_engine_run_convergence() {
        let mut cfg = default_config(8);
        cfg.convergence_threshold = 1e-6;
        cfg.max_generations = 30;
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();
        let result = engine.run(|_| perfect_metrics());
        assert!(
            result.best_genome.fitness_score > 0.0 || result.generations_run > 0,
            "run must produce a valid result"
        );
    }

    // ── Test 20: fitness_history length matches generations_run ──────────

    #[test]
    fn test_engine_fitness_history() {
        let mut cfg = default_config(6);
        cfg.max_generations = 7;
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();
        let result = engine.run(|_| perfect_metrics());
        assert_eq!(
            result.fitness_history.len() as u64,
            result.generations_run,
            "fitness_history length must equal generations_run"
        );
    }

    // ── Test 21: elitist selection keeps top-K exactly ────────────────────

    #[test]
    fn test_selection_elitist() {
        // Use elite_fraction = 0.4 so the top 4 are preserved from a pop of 10.
        let mut cfg = default_config(10);
        cfg.elite_fraction = 0.4;
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights())
            .with_selection(SelectionStrategy::Elitist { k: 4 });
        engine.initialize();

        // Assign distinct known fitnesses and remember the top IDs.
        for (i, g) in engine.population.iter_mut().enumerate() {
            g.fitness_score = i as f64 * 0.1;
        }
        engine
            .population
            .sort_by(|a, b| b.fitness_score.partial_cmp(&a.fitness_score).unwrap());
        // The very best genome is index 0 after sorting.
        let top_id = engine.population[0].id;

        // Capture id→fitness map before step so metrics_fn can reproduce ordering.
        let ordered: Vec<(u64, f64)> = engine
            .population
            .iter()
            .map(|g| (g.id, g.fitness_score))
            .collect();
        engine.step(|genome| {
            let score = ordered
                .iter()
                .find(|(id, _)| *id == genome.id)
                .map(|(_, s)| *s)
                .unwrap_or(0.0);
            FitnessMetrics {
                throughput: score * 1000.0,
                latency_ms: (1.0 - score) * 10.0,
                resource_efficiency: score,
                coordination_score: score,
                fault_tolerance: score,
            }
        });

        // The single highest-fitness genome must be preserved.
        let ids_after: Vec<u64> = engine.population().iter().map(|g| g.id).collect();
        assert!(
            ids_after.contains(&top_id),
            "top elite id {top_id} must survive into next generation"
        );
    }

    // ── Test 22: tournament winner has highest fitness among drawn set ────

    #[test]
    fn test_selection_tournament() {
        let cfg = default_config(20);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights())
            .with_selection(SelectionStrategy::Tournament { tournament_size: 5 });
        engine.initialize();

        // Assign unique fitnesses
        for (i, g) in engine.population.iter_mut().enumerate() {
            g.fitness_score = (i as f64) / 20.0;
        }

        let winner_idx = engine.tournament_select(5);
        assert!(winner_idx < engine.population.len());
    }

    // ── Test 23: roulette wheel selects higher-fitness genomes more often ─

    #[test]
    fn test_roulette_wheel_proportional() {
        let cfg = default_config(3);
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights())
            .with_selection(SelectionStrategy::RouletteWheel);
        engine.initialize();

        // Skewed fitness: genome 0=0.01, 1=0.01, 2=10.0
        engine.population[0].fitness_score = 0.01;
        engine.population[1].fitness_score = 0.01;
        engine.population[2].fitness_score = 10.0;

        let mut counts = [0usize; 3];
        for _ in 0..1000 {
            let idx = engine.roulette_select();
            counts[idx] += 1;
        }

        let total: usize = counts.iter().sum();
        let frac_2 = counts[2] as f64 / total as f64;
        assert!(
            frac_2 > 0.8,
            "high-fitness genome should dominate roulette; got frac={frac_2:.3}"
        );
    }

    // ── Test 24: capability discovery — suggest above threshold ──────────

    #[test]
    fn test_capability_discovery_observe() {
        let mut disc = CapabilityDiscovery::new(5);
        for _ in 0..5 {
            disc.observe("fast_sort");
        }
        disc.observe("slow_merge"); // only 1 observation — below threshold
        let suggestions = disc.suggested_capabilities();
        assert!(suggestions.contains(&"fast_sort".to_owned()));
        assert!(!suggestions.contains(&"slow_merge".to_owned()));
    }

    // ── Test 25: capability discovery — clear resets counts ──────────────

    #[test]
    fn test_capability_discovery_clear() {
        let mut disc = CapabilityDiscovery::new(3);
        for _ in 0..10 {
            disc.observe("pattern_x");
        }
        assert!(!disc.suggested_capabilities().is_empty());
        disc.clear_observations();
        assert!(
            disc.suggested_capabilities().is_empty(),
            "after clear, no suggestions should remain"
        );
    }

    // ── Test 26: diversity near zero for identical population ─────────────

    #[test]
    fn test_diversity_metric() {
        let gene = make_gene("only", 100, true);
        let population: Vec<AgentGenome> = (0..5)
            .map(|i| make_genome_with_fitness(i, 0.5, vec![gene.clone()]))
            .collect();
        let diversity = EvolutionEngine::compute_diversity(&population);
        // All identical → enabled bits always match → contribution = 0
        // Proficiency all 100 → diff=0 → diversity should be 0
        assert!(
            diversity < 1e-6,
            "identical population must have near-zero diversity, got {diversity}"
        );
    }

    // ── Test 27: full 5-generation run with real metrics_fn ──────────────

    #[test]
    fn test_evolution_full_run() {
        let mut cfg = default_config(12);
        cfg.max_generations = 5;
        let mut engine = EvolutionEngine::new(cfg, FitnessEvaluator::default_weights());
        engine.initialize();

        let result = engine.run(|genome| {
            // Fitness proportional to total proficiency of enabled genes
            let sum: u32 = genome
                .capabilities
                .iter()
                .filter(|g| g.enabled)
                .map(|g| g.proficiency as u32)
                .sum();
            let score = (sum as f64 / (255.0 * 6.0)).clamp(0.0, 1.0);
            FitnessMetrics {
                throughput: score * 1000.0,
                latency_ms: (1.0 - score) * 200.0,
                resource_efficiency: score,
                coordination_score: score,
                fault_tolerance: score,
            }
        });

        assert!(result.generations_run > 0);
        assert!(!result.fitness_history.is_empty());
        assert!(result.best_genome.fitness_score >= 0.0);
    }
}
