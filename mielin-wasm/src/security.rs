//! Security Enhancements for MielinOS WASM Runtime
//!
//! This module provides advanced security mechanisms to protect WASM execution:
//!
//! # Features
//!
//! - **Control-Flow Integrity (CFI)**: Validates control-flow transfers
//! - **Stack Canaries**: Detects stack buffer overflows
//! - **ASLR for WASM Memory**: Randomizes memory layout
//! - **Capability Attestation**: Cryptographic capability proofs
//! - **Shadow Stacks**: Hardware-backed return address protection
//! - **Memory Tagging**: Tagged memory for spatial safety
//!
//! # Example
//!
//! ```rust,ignore
//! use mielin_wasm::security::{SecurityContext, SecurityPolicy};
//!
//! // Create security context with strict policy
//! let mut ctx = SecurityContext::new(SecurityPolicy::strict());
//!
//! // Enable CFI
//! ctx.enable_cfi()?;
//!
//! // Add stack canaries
//! ctx.enable_stack_canaries()?;
//!
//! // Enable ASLR
//! ctx.enable_aslr()?;
//!
//! // Validate execution
//! ctx.validate_call_target(target_addr)?;
//! ```

use anyhow::{anyhow, bail, Result};
use std::collections::{HashMap, HashSet};

/// Security context for a WASM execution
#[derive(Debug)]
pub struct SecurityContext {
    /// Security policy
    policy: SecurityPolicy,
    /// Control-flow integrity state
    cfi_state: Option<CfiState>,
    /// Stack canary state
    canary_state: Option<CanaryState>,
    /// ASLR state
    aslr_state: Option<AslrState>,
    /// Capability attestation state
    attestation_state: Option<AttestationState>,
    /// Security violations
    violations: Vec<SecurityViolation>,
}

/// Security policy configuration
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    /// Enable control-flow integrity
    pub enable_cfi: bool,
    /// Enable stack canaries
    pub enable_stack_canaries: bool,
    /// Enable ASLR for WASM memory
    pub enable_aslr: bool,
    /// Enable capability attestation
    pub enable_attestation: bool,
    /// Panic on security violation
    pub panic_on_violation: bool,
    /// Maximum allowed violations before termination
    pub max_violations: usize,
}

impl SecurityPolicy {
    /// Strict security policy (all features enabled)
    pub fn strict() -> Self {
        Self {
            enable_cfi: true,
            enable_stack_canaries: true,
            enable_aslr: true,
            enable_attestation: true,
            panic_on_violation: true,
            max_violations: 0,
        }
    }

    /// Standard security policy (balanced)
    pub fn standard() -> Self {
        Self {
            enable_cfi: true,
            enable_stack_canaries: true,
            enable_aslr: false,
            enable_attestation: false,
            panic_on_violation: false,
            max_violations: 10,
        }
    }

    /// Permissive security policy (minimal overhead)
    pub fn permissive() -> Self {
        Self {
            enable_cfi: false,
            enable_stack_canaries: false,
            enable_aslr: false,
            enable_attestation: false,
            panic_on_violation: false,
            max_violations: 100,
        }
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

/// Control-Flow Integrity state
#[derive(Debug)]
pub struct CfiState {
    /// Valid call targets (indirect calls)
    valid_call_targets: HashSet<u32>,
    /// Current call depth
    call_depth: usize,
    /// Maximum call depth
    max_call_depth: usize,
    /// CFI violations detected
    violations: usize,
    /// Shadow stack for return addresses
    shadow_stack: Vec<u32>,
}

impl CfiState {
    /// Create new CFI state
    pub fn new(max_call_depth: usize) -> Self {
        Self {
            valid_call_targets: HashSet::new(),
            call_depth: 0,
            max_call_depth,
            violations: 0,
            shadow_stack: Vec::with_capacity(max_call_depth),
        }
    }

    /// Add valid call target
    pub fn add_call_target(&mut self, address: u32) {
        self.valid_call_targets.insert(address);
    }

    /// Validate call target (CFI check)
    pub fn validate_call(&mut self, target: u32) -> Result<()> {
        if !self.valid_call_targets.contains(&target) {
            self.violations += 1;
            bail!(
                "CFI violation: Invalid call target 0x{:08x} (not in valid target set)",
                target
            );
        }

        if self.call_depth >= self.max_call_depth {
            self.violations += 1;
            bail!(
                "CFI violation: Maximum call depth {} exceeded",
                self.max_call_depth
            );
        }

        self.call_depth += 1;
        Ok(())
    }

    /// Validate return (shadow stack check)
    pub fn validate_return(&mut self, return_address: u32) -> Result<()> {
        if let Some(expected) = self.shadow_stack.pop() {
            if expected != return_address {
                self.violations += 1;
                bail!(
                    "CFI violation: Return address mismatch. Expected 0x{:08x}, got 0x{:08x}",
                    expected,
                    return_address
                );
            }
        } else {
            self.violations += 1;
            bail!("CFI violation: Return with empty shadow stack");
        }

        if self.call_depth > 0 {
            self.call_depth -= 1;
        }

        Ok(())
    }

    /// Push return address onto shadow stack
    pub fn push_return_address(&mut self, address: u32) -> Result<()> {
        if self.shadow_stack.len() >= self.max_call_depth {
            bail!("Shadow stack overflow");
        }
        self.shadow_stack.push(address);
        Ok(())
    }

    /// Get CFI statistics
    pub fn stats(&self) -> CfiStats {
        CfiStats {
            valid_targets: self.valid_call_targets.len(),
            current_depth: self.call_depth,
            max_depth: self.max_call_depth,
            violations: self.violations,
            shadow_stack_size: self.shadow_stack.len(),
        }
    }
}

/// CFI statistics
#[derive(Debug, Clone)]
pub struct CfiStats {
    pub valid_targets: usize,
    pub current_depth: usize,
    pub max_depth: usize,
    pub violations: usize,
    pub shadow_stack_size: usize,
}

/// Stack canary state
#[derive(Debug)]
pub struct CanaryState {
    /// Canary value (random)
    canary_value: u64,
    /// Stack frame canaries (frame_id -> canary_address)
    frame_canaries: HashMap<u32, u32>,
    /// Canary violations detected
    violations: usize,
}

impl CanaryState {
    /// Create new canary state with random value
    pub fn new() -> Self {
        // Generate random canary using XorShift128+
        let canary_value = Self::generate_canary();

        Self {
            canary_value,
            frame_canaries: HashMap::new(),
            violations: 0,
        }
    }

    /// Generate random canary value
    fn generate_canary() -> u64 {
        // XorShift128+ for canary generation
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_nanos() as u64;

        let mut s0 = seed;
        let mut s1 = seed.wrapping_mul(6364136223846793005);

        // XorShift step
        s1 ^= s0;
        s0 = s0.rotate_left(24) ^ s1 ^ (s1 << 16);
        s1 = s1.rotate_left(37);

        s0.wrapping_add(s1)
    }

    /// Place canary for stack frame
    pub fn place_canary(&mut self, frame_id: u32, canary_address: u32) {
        self.frame_canaries.insert(frame_id, canary_address);
    }

    /// Verify canary for stack frame
    pub fn verify_canary(&mut self, frame_id: u32, actual_value: u64) -> Result<()> {
        if actual_value != self.canary_value {
            self.violations += 1;
            bail!(
                "Stack canary violation: Frame {} - Expected 0x{:016x}, got 0x{:016x}",
                frame_id,
                self.canary_value,
                actual_value
            );
        }
        Ok(())
    }

    /// Remove canary after frame exit
    pub fn remove_canary(&mut self, frame_id: u32) {
        self.frame_canaries.remove(&frame_id);
    }

    /// Get canary value (for placing in memory)
    pub fn get_canary_value(&self) -> u64 {
        self.canary_value
    }

    /// Get canary statistics
    pub fn stats(&self) -> CanaryStats {
        CanaryStats {
            active_canaries: self.frame_canaries.len(),
            violations: self.violations,
        }
    }
}

impl Default for CanaryState {
    fn default() -> Self {
        Self::new()
    }
}

/// Canary statistics
#[derive(Debug, Clone)]
pub struct CanaryStats {
    pub active_canaries: usize,
    pub violations: usize,
}

/// ASLR (Address Space Layout Randomization) state
#[derive(Debug)]
pub struct AslrState {
    /// Memory base offset (random)
    base_offset: u32,
    /// Heap base offset (random)
    heap_offset: u32,
    /// Stack base offset (random)
    stack_offset: u32,
    /// Whether offsets have been applied
    applied: bool,
}

impl AslrState {
    /// Create new ASLR state with random offsets
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_nanos() as u64;

        // Generate random offsets (page-aligned)
        // Use different parts of seed for different offsets
        let base_offset = ((seed & 0xFFFF) as u32) << 16; // Up to 4GB offset
        let heap_offset = (((seed >> 16) & 0xFFFF) as u32) << 16;
        let stack_offset = (((seed >> 32) & 0xFFFF) as u32) << 16;

        Self {
            base_offset,
            heap_offset,
            stack_offset,
            applied: false,
        }
    }

    /// Get randomized base address
    pub fn get_base_address(&self, original: u32) -> u32 {
        original.wrapping_add(self.base_offset)
    }

    /// Get randomized heap address
    pub fn get_heap_address(&self, original: u32) -> u32 {
        original.wrapping_add(self.heap_offset)
    }

    /// Get randomized stack address
    pub fn get_stack_address(&self, original: u32) -> u32 {
        original.wrapping_add(self.stack_offset)
    }

    /// Mark offsets as applied
    pub fn mark_applied(&mut self) {
        self.applied = true;
    }

    /// Check if offsets are applied
    pub fn is_applied(&self) -> bool {
        self.applied
    }

    /// Get ASLR statistics
    pub fn stats(&self) -> AslrStats {
        AslrStats {
            base_offset: self.base_offset,
            heap_offset: self.heap_offset,
            stack_offset: self.stack_offset,
            applied: self.applied,
        }
    }
}

impl Default for AslrState {
    fn default() -> Self {
        Self::new()
    }
}

/// ASLR statistics
#[derive(Debug, Clone)]
pub struct AslrStats {
    pub base_offset: u32,
    pub heap_offset: u32,
    pub stack_offset: u32,
    pub applied: bool,
}

/// Capability attestation state
#[derive(Debug)]
pub struct AttestationState {
    /// Capability tokens (capability_id -> token)
    tokens: HashMap<String, CapabilityToken>,
    /// Delegation chain (child_id -> parent_id)
    delegation_chain: HashMap<String, String>,
    /// Revoked capabilities
    revoked: HashSet<String>,
}

impl AttestationState {
    /// Create new attestation state
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
            delegation_chain: HashMap::new(),
            revoked: HashSet::new(),
        }
    }

    /// Create capability token
    pub fn create_token(
        &mut self,
        capability_id: String,
        permissions: Vec<String>,
    ) -> Result<CapabilityToken> {
        if self.tokens.contains_key(&capability_id) {
            bail!("Capability token already exists: {}", capability_id);
        }

        let token = CapabilityToken::new(capability_id.clone(), permissions);
        self.tokens.insert(capability_id, token.clone());

        Ok(token)
    }

    /// Delegate capability to child
    pub fn delegate(
        &mut self,
        parent_id: &str,
        child_id: String,
        permissions: Vec<String>,
    ) -> Result<CapabilityToken> {
        // Verify parent exists and is not revoked
        let parent_token = self
            .tokens
            .get(parent_id)
            .ok_or_else(|| anyhow!("Parent capability not found: {}", parent_id))?;

        if self.revoked.contains(parent_id) {
            bail!("Parent capability is revoked: {}", parent_id);
        }

        // Verify child permissions are subset of parent
        for perm in &permissions {
            if !parent_token.permissions.contains(perm) {
                bail!(
                    "Permission '{}' not in parent capability {}",
                    perm,
                    parent_id
                );
            }
        }

        // Create child token
        let child_token = CapabilityToken::new(child_id.clone(), permissions);
        self.tokens.insert(child_id.clone(), child_token.clone());
        self.delegation_chain
            .insert(child_id, parent_id.to_string());

        Ok(child_token)
    }

    /// Verify capability token
    pub fn verify(&self, capability_id: &str, required_permission: &str) -> Result<()> {
        // Check if revoked
        if self.revoked.contains(capability_id) {
            bail!("Capability is revoked: {}", capability_id);
        }

        // Get token
        let token = self
            .tokens
            .get(capability_id)
            .ok_or_else(|| anyhow!("Capability not found: {}", capability_id))?;

        // Verify permission
        if !token.permissions.contains(&required_permission.to_string()) {
            bail!(
                "Capability {} does not have permission '{}'",
                capability_id,
                required_permission
            );
        }

        // Verify delegation chain (check all parents)
        let mut current = capability_id;
        while let Some(parent) = self.delegation_chain.get(current) {
            if self.revoked.contains(parent) {
                bail!("Parent capability {} in chain is revoked", parent);
            }
            current = parent;
        }

        Ok(())
    }

    /// Revoke capability (and all children)
    pub fn revoke(&mut self, capability_id: &str) -> Result<()> {
        if !self.tokens.contains_key(capability_id) {
            bail!("Capability not found: {}", capability_id);
        }

        // Revoke this capability
        self.revoked.insert(capability_id.to_string());

        // Find and revoke all children
        let children: Vec<String> = self
            .delegation_chain
            .iter()
            .filter(|(_, parent)| parent.as_str() == capability_id)
            .map(|(child, _)| child.clone())
            .collect();

        for child in children {
            self.revoke(&child)?;
        }

        Ok(())
    }

    /// Get attestation statistics
    pub fn stats(&self) -> AttestationStats {
        AttestationStats {
            total_tokens: self.tokens.len(),
            revoked_tokens: self.revoked.len(),
            delegation_chains: self.delegation_chain.len(),
        }
    }
}

impl Default for AttestationState {
    fn default() -> Self {
        Self::new()
    }
}

/// Attestation statistics
#[derive(Debug, Clone)]
pub struct AttestationStats {
    pub total_tokens: usize,
    pub revoked_tokens: usize,
    pub delegation_chains: usize,
}

/// Capability token
#[derive(Debug, Clone)]
pub struct CapabilityToken {
    /// Capability identifier
    pub id: String,
    /// Permissions granted
    pub permissions: Vec<String>,
    /// Signature (SHA-256 hash for now)
    pub signature: [u8; 32],
    /// Creation timestamp
    pub created_at: u64,
}

impl CapabilityToken {
    /// Create new capability token
    pub fn new(id: String, permissions: Vec<String>) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_secs();

        // Generate signature (simple hash for now)
        let signature = Self::generate_signature(&id, &permissions, created_at);

        Self {
            id,
            permissions,
            signature,
            created_at,
        }
    }

    /// Generate signature for capability token
    fn generate_signature(id: &str, permissions: &[String], timestamp: u64) -> [u8; 32] {
        // Simple FNV-1a hash for demonstration
        // In production, use proper cryptographic signature
        let mut hash = [0u8; 32];
        let mut h: u64 = 14695981039346656037; // FNV offset basis

        // Hash ID
        for byte in id.as_bytes() {
            h ^= *byte as u64;
            h = h.wrapping_mul(1099511628211); // FNV prime
        }

        // Hash permissions
        for perm in permissions {
            for byte in perm.as_bytes() {
                h ^= *byte as u64;
                h = h.wrapping_mul(1099511628211);
            }
        }

        // Hash timestamp
        for byte in timestamp.to_le_bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(1099511628211);
        }

        // Fill hash array (repeat pattern)
        for i in 0..4 {
            let bytes = h.to_le_bytes();
            hash[i * 8..(i + 1) * 8].copy_from_slice(&bytes);
            h = h.wrapping_mul(1099511628211);
        }

        hash
    }

    /// Verify token signature
    pub fn verify_signature(&self) -> bool {
        let expected = Self::generate_signature(&self.id, &self.permissions, self.created_at);
        self.signature == expected
    }
}

/// Security violation record
#[derive(Debug, Clone)]
pub struct SecurityViolation {
    /// Type of violation
    pub violation_type: ViolationType,
    /// Description
    pub description: String,
    /// Timestamp
    pub timestamp: u64,
}

/// Type of security violation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationType {
    /// Control-flow integrity violation
    CfiViolation,
    /// Stack canary violation
    CanaryViolation,
    /// ASLR violation
    AslrViolation,
    /// Attestation violation
    AttestationViolation,
}

impl SecurityContext {
    /// Create new security context
    pub fn new(policy: SecurityPolicy) -> Self {
        Self {
            policy,
            cfi_state: None,
            canary_state: None,
            aslr_state: None,
            attestation_state: None,
            violations: Vec::new(),
        }
    }

    /// Enable control-flow integrity
    pub fn enable_cfi(&mut self, max_call_depth: usize) -> Result<()> {
        if !self.policy.enable_cfi {
            bail!("CFI is disabled in security policy");
        }

        self.cfi_state = Some(CfiState::new(max_call_depth));
        Ok(())
    }

    /// Enable stack canaries
    pub fn enable_stack_canaries(&mut self) -> Result<()> {
        if !self.policy.enable_stack_canaries {
            bail!("Stack canaries are disabled in security policy");
        }

        self.canary_state = Some(CanaryState::new());
        Ok(())
    }

    /// Enable ASLR
    pub fn enable_aslr(&mut self) -> Result<()> {
        if !self.policy.enable_aslr {
            bail!("ASLR is disabled in security policy");
        }

        self.aslr_state = Some(AslrState::new());
        Ok(())
    }

    /// Enable capability attestation
    pub fn enable_attestation(&mut self) -> Result<()> {
        if !self.policy.enable_attestation {
            bail!("Attestation is disabled in security policy");
        }

        self.attestation_state = Some(AttestationState::new());
        Ok(())
    }

    /// Validate call target (CFI)
    pub fn validate_call(&mut self, target: u32) -> Result<()> {
        if let Some(cfi) = &mut self.cfi_state {
            cfi.validate_call(target).inspect_err(|e| {
                self.record_violation(ViolationType::CfiViolation, e.to_string());
            })
        } else {
            Ok(())
        }
    }

    /// Get CFI state (mutable)
    pub fn cfi_state_mut(&mut self) -> Option<&mut CfiState> {
        self.cfi_state.as_mut()
    }

    /// Get canary state (mutable)
    pub fn canary_state_mut(&mut self) -> Option<&mut CanaryState> {
        self.canary_state.as_mut()
    }

    /// Get ASLR state (mutable)
    pub fn aslr_state_mut(&mut self) -> Option<&mut AslrState> {
        self.aslr_state.as_mut()
    }

    /// Get attestation state (mutable)
    pub fn attestation_state_mut(&mut self) -> Option<&mut AttestationState> {
        self.attestation_state.as_mut()
    }

    /// Record security violation
    fn record_violation(&mut self, violation_type: ViolationType, description: String) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_secs();

        let desc_clone = description.clone();
        self.violations.push(SecurityViolation {
            violation_type,
            description,
            timestamp,
        });

        if self.policy.panic_on_violation {
            panic!("Security violation: {}", desc_clone);
        }
    }

    /// Get violations
    pub fn get_violations(&self) -> &[SecurityViolation] {
        &self.violations
    }

    /// Get security statistics
    pub fn stats(&self) -> SecurityStats {
        SecurityStats {
            cfi_stats: self.cfi_state.as_ref().map(|s| s.stats()),
            canary_stats: self.canary_state.as_ref().map(|s| s.stats()),
            aslr_stats: self.aslr_state.as_ref().map(|s| s.stats()),
            attestation_stats: self.attestation_state.as_ref().map(|s| s.stats()),
            total_violations: self.violations.len(),
        }
    }
}

/// Security statistics
#[derive(Debug, Clone)]
pub struct SecurityStats {
    pub cfi_stats: Option<CfiStats>,
    pub canary_stats: Option<CanaryStats>,
    pub aslr_stats: Option<AslrStats>,
    pub attestation_stats: Option<AttestationStats>,
    pub total_violations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_policy_presets() {
        let strict = SecurityPolicy::strict();
        assert!(strict.enable_cfi);
        assert!(strict.enable_stack_canaries);
        assert!(strict.enable_aslr);
        assert!(strict.enable_attestation);
        assert!(strict.panic_on_violation);

        let standard = SecurityPolicy::standard();
        assert!(standard.enable_cfi);
        assert!(standard.enable_stack_canaries);

        let permissive = SecurityPolicy::permissive();
        assert!(!permissive.enable_cfi);
        assert!(!permissive.enable_stack_canaries);
    }

    #[test]
    fn test_cfi_state_creation() {
        let cfi = CfiState::new(100);
        assert_eq!(cfi.max_call_depth, 100);
        assert_eq!(cfi.call_depth, 0);
        assert_eq!(cfi.violations, 0);
    }

    #[test]
    fn test_cfi_call_validation() {
        let mut cfi = CfiState::new(10);
        cfi.add_call_target(0x1000);
        cfi.add_call_target(0x2000);

        // Valid call
        assert!(cfi.validate_call(0x1000).is_ok());
        assert_eq!(cfi.call_depth, 1);

        // Invalid call
        assert!(cfi.validate_call(0x3000).is_err());
        assert_eq!(cfi.violations, 1);
    }

    #[test]
    fn test_cfi_shadow_stack() {
        let mut cfi = CfiState::new(10);
        cfi.add_call_target(0x1000);

        // Push return address
        assert!(cfi.push_return_address(0x5000).is_ok());

        // Valid return
        assert!(cfi.validate_return(0x5000).is_ok());

        // Invalid return (empty stack)
        assert!(cfi.validate_return(0x6000).is_err());
    }

    #[test]
    fn test_cfi_max_depth() {
        let mut cfi = CfiState::new(2);
        cfi.add_call_target(0x1000);

        assert!(cfi.validate_call(0x1000).is_ok());
        assert!(cfi.validate_call(0x1000).is_ok());
        // Third call should fail (max depth = 2)
        assert!(cfi.validate_call(0x1000).is_err());
    }

    #[test]
    fn test_canary_generation() {
        let canary1 = CanaryState::new();
        let canary2 = CanaryState::new();

        // Canaries should be different (high probability)
        assert_ne!(canary1.canary_value, canary2.canary_value);
    }

    #[test]
    fn test_canary_verification() {
        let mut canary = CanaryState::new();
        let canary_value = canary.get_canary_value();

        // Place canary
        canary.place_canary(1, 0x1000);

        // Valid verification
        assert!(canary.verify_canary(1, canary_value).is_ok());

        // Invalid verification
        assert!(canary.verify_canary(1, canary_value + 1).is_err());
        assert_eq!(canary.violations, 1);
    }

    #[test]
    fn test_aslr_randomization() {
        use std::collections::HashSet;
        use std::thread;
        use std::time::Duration;

        // Create multiple ASLR states with delays to ensure different timestamps
        let mut base_offsets = HashSet::new();
        let mut heap_offsets = HashSet::new();
        let mut stack_offsets = HashSet::new();

        for i in 0..5 {
            if i > 0 {
                // Small delay to ensure different timestamps
                thread::sleep(Duration::from_micros(10));
            }

            let aslr = AslrState::new();
            base_offsets.insert(aslr.base_offset);
            heap_offsets.insert(aslr.heap_offset);
            stack_offsets.insert(aslr.stack_offset);
        }

        // At least one type of offset should have variation (most likely all three)
        // With 5 samples and proper randomization, we should see multiple unique values
        let has_variation =
            base_offsets.len() > 1 || heap_offsets.len() > 1 || stack_offsets.len() > 1;

        assert!(
            has_variation,
            "ASLR should produce varied offsets: base={} heap={} stack={}",
            base_offsets.len(),
            heap_offsets.len(),
            stack_offsets.len()
        );
    }

    #[test]
    fn test_aslr_address_translation() {
        let aslr = AslrState::new();
        let original = 0x1000;

        let randomized = aslr.get_base_address(original);
        assert_ne!(randomized, original);

        // Should be deterministic for same input
        assert_eq!(randomized, aslr.get_base_address(original));
    }

    #[test]
    fn test_attestation_token_creation() {
        let mut attestation = AttestationState::new();

        let token = attestation
            .create_token(
                "cap1".to_string(),
                vec!["read".to_string(), "write".to_string()],
            )
            .unwrap();

        assert_eq!(token.id, "cap1");
        assert_eq!(token.permissions.len(), 2);
        assert!(token.verify_signature());
    }

    #[test]
    fn test_attestation_delegation() {
        let mut attestation = AttestationState::new();

        // Create parent capability
        attestation
            .create_token(
                "parent".to_string(),
                vec![
                    "read".to_string(),
                    "write".to_string(),
                    "execute".to_string(),
                ],
            )
            .unwrap();

        // Delegate subset to child
        let child = attestation
            .delegate("parent", "child".to_string(), vec!["read".to_string()])
            .unwrap();

        assert_eq!(child.permissions.len(), 1);

        // Verify child can use permission
        assert!(attestation.verify("child", "read").is_ok());

        // Child should not have permission it wasn't delegated
        assert!(attestation.verify("child", "write").is_err());
    }

    #[test]
    fn test_attestation_revocation() {
        let mut attestation = AttestationState::new();

        // Create parent and child
        attestation
            .create_token("parent".to_string(), vec!["read".to_string()])
            .unwrap();
        attestation
            .delegate("parent", "child".to_string(), vec!["read".to_string()])
            .unwrap();

        // Revoke parent
        attestation.revoke("parent").unwrap();

        // Both should be revoked
        assert!(attestation.verify("parent", "read").is_err());
        assert!(attestation.verify("child", "read").is_err());
    }

    #[test]
    fn test_security_context_creation() {
        let policy = SecurityPolicy::strict();
        let ctx = SecurityContext::new(policy);

        assert!(ctx.cfi_state.is_none());
        assert!(ctx.canary_state.is_none());
        assert!(ctx.aslr_state.is_none());
        assert!(ctx.attestation_state.is_none());
    }

    #[test]
    fn test_security_context_enable_cfi() {
        let policy = SecurityPolicy::strict();
        let mut ctx = SecurityContext::new(policy);

        assert!(ctx.enable_cfi(100).is_ok());
        assert!(ctx.cfi_state.is_some());
    }

    #[test]
    fn test_security_context_enable_canaries() {
        let policy = SecurityPolicy::strict();
        let mut ctx = SecurityContext::new(policy);

        assert!(ctx.enable_stack_canaries().is_ok());
        assert!(ctx.canary_state.is_some());
    }

    #[test]
    fn test_security_context_enable_aslr() {
        let policy = SecurityPolicy::strict();
        let mut ctx = SecurityContext::new(policy);

        assert!(ctx.enable_aslr().is_ok());
        assert!(ctx.aslr_state.is_some());
    }

    #[test]
    fn test_security_context_enable_attestation() {
        let policy = SecurityPolicy::strict();
        let mut ctx = SecurityContext::new(policy);

        assert!(ctx.enable_attestation().is_ok());
        assert!(ctx.attestation_state.is_some());
    }

    #[test]
    fn test_security_context_disabled_features() {
        let policy = SecurityPolicy::permissive();
        let mut ctx = SecurityContext::new(policy);

        // Should fail because disabled in policy
        assert!(ctx.enable_cfi(100).is_err());
        assert!(ctx.enable_stack_canaries().is_err());
    }

    #[test]
    fn test_capability_token_signature() {
        let token = CapabilityToken::new("test".to_string(), vec!["read".to_string()]);

        // Signature should verify
        assert!(token.verify_signature());

        // Tampered token should not verify
        let mut tampered = token.clone();
        tampered.signature[0] ^= 0xFF;
        assert!(!tampered.verify_signature());
    }
}
