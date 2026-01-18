//! Integration tests for security enhancements

use mielin_wasm::security::{SecurityContext, SecurityPolicy, ViolationType};

#[test]
fn test_security_context_creation() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    assert!(ctx.cfi_state_mut().is_none());
    assert!(ctx.canary_state_mut().is_none());
    assert!(ctx.aslr_state_mut().is_none());
    assert!(ctx.attestation_state_mut().is_none());
}

#[test]
fn test_cfi_enable_and_validate() {
    let mut policy = SecurityPolicy::strict();
    policy.panic_on_violation = false; // Don't panic on violations in tests
    let mut ctx = SecurityContext::new(policy);

    // Enable CFI
    assert!(ctx.enable_cfi(100).is_ok());

    // Add valid call targets
    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);
        cfi.add_call_target(0x2000);
        cfi.add_call_target(0x3000);
    }

    // Validate valid call
    assert!(ctx.validate_call(0x1000).is_ok());

    // Validate invalid call
    let result = ctx.validate_call(0x9000);
    assert!(result.is_err());

    // Check violations recorded
    let violations = ctx.get_violations();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].violation_type, ViolationType::CfiViolation);
}

#[test]
fn test_cfi_shadow_stack() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_cfi(10).unwrap();

    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);

        // Push return addresses
        assert!(cfi.push_return_address(0x5000).is_ok());
        assert!(cfi.push_return_address(0x6000).is_ok());

        // Validate returns in LIFO order
        assert!(cfi.validate_return(0x6000).is_ok());
        assert!(cfi.validate_return(0x5000).is_ok());

        // Stack should be empty now
        let result = cfi.validate_return(0x7000);
        assert!(result.is_err());
    }
}

#[test]
fn test_cfi_max_call_depth() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_cfi(3).unwrap();

    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);

        // Should allow up to max_depth calls
        assert!(cfi.validate_call(0x1000).is_ok());
        assert!(cfi.validate_call(0x1000).is_ok());
        assert!(cfi.validate_call(0x1000).is_ok());

        // Fourth call should fail
        assert!(cfi.validate_call(0x1000).is_err());
    }
}

#[test]
fn test_cfi_stats() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_cfi(100).unwrap();

    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);
        cfi.add_call_target(0x2000);

        cfi.validate_call(0x1000).ok();
        cfi.validate_call(0x9000).ok(); // Invalid, creates violation

        let stats = cfi.stats();
        assert_eq!(stats.valid_targets, 2);
        assert_eq!(stats.violations, 1);
        assert_eq!(stats.current_depth, 1); // Only one valid call
    }
}

#[test]
fn test_stack_canaries_enable() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    assert!(ctx.enable_stack_canaries().is_ok());
    assert!(ctx.canary_state_mut().is_some());
}

#[test]
fn test_stack_canaries_placement_and_verification() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_stack_canaries().unwrap();

    if let Some(canary) = ctx.canary_state_mut() {
        let canary_value = canary.get_canary_value();

        // Place canaries for multiple frames
        canary.place_canary(1, 0x1000);
        canary.place_canary(2, 0x2000);
        canary.place_canary(3, 0x3000);

        // Verify valid canaries
        assert!(canary.verify_canary(1, canary_value).is_ok());
        assert!(canary.verify_canary(2, canary_value).is_ok());
        assert!(canary.verify_canary(3, canary_value).is_ok());

        // Verify invalid canary (buffer overflow simulation)
        let result = canary.verify_canary(1, canary_value ^ 0xFF);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Stack canary violation"));

        let stats = canary.stats();
        assert_eq!(stats.active_canaries, 3);
        assert_eq!(stats.violations, 1);
    }
}

#[test]
fn test_stack_canaries_randomization() {
    let policy = SecurityPolicy::strict();
    let mut ctx1 = SecurityContext::new(policy.clone());
    let mut ctx2 = SecurityContext::new(policy);

    ctx1.enable_stack_canaries().unwrap();
    ctx2.enable_stack_canaries().unwrap();

    let canary1 = ctx1.canary_state_mut().unwrap().get_canary_value();
    let canary2 = ctx2.canary_state_mut().unwrap().get_canary_value();

    // Canaries should be different (very high probability)
    assert_ne!(canary1, canary2);
}

#[test]
fn test_stack_canaries_removal() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_stack_canaries().unwrap();

    if let Some(canary) = ctx.canary_state_mut() {
        canary.place_canary(1, 0x1000);
        canary.place_canary(2, 0x2000);

        assert_eq!(canary.stats().active_canaries, 2);

        // Remove one canary
        canary.remove_canary(1);
        assert_eq!(canary.stats().active_canaries, 1);

        // Remove another
        canary.remove_canary(2);
        assert_eq!(canary.stats().active_canaries, 0);
    }
}

#[test]
fn test_aslr_enable() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    assert!(ctx.enable_aslr().is_ok());
    assert!(ctx.aslr_state_mut().is_some());
}

#[test]
fn test_aslr_randomization() {
    use std::collections::HashSet;

    let policy = SecurityPolicy::strict();
    let mut base_offsets = HashSet::new();
    let mut heap_offsets = HashSet::new();
    let mut stack_offsets = HashSet::new();

    // Create multiple ASLR states with delays to ensure different timestamps
    for i in 0..5 {
        if i > 0 {
            // Small delay to ensure different timestamps
            std::thread::sleep(std::time::Duration::from_micros(10));
        }

        let mut ctx = SecurityContext::new(policy.clone());
        ctx.enable_aslr().unwrap();

        if let Some(aslr) = ctx.aslr_state_mut() {
            let stats = aslr.stats();
            base_offsets.insert(stats.base_offset);
            heap_offsets.insert(stats.heap_offset);
            stack_offsets.insert(stats.stack_offset);
        }
    }

    // At least one type of offset should have variation (most likely all three)
    // With 5 samples and proper randomization, we should see multiple unique values
    let has_variation = base_offsets.len() > 1 || heap_offsets.len() > 1 || stack_offsets.len() > 1;

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
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_aslr().unwrap();

    if let Some(aslr) = ctx.aslr_state_mut() {
        let original_base = 0x10000;
        let original_heap = 0x20000;
        let original_stack = 0x30000;

        let randomized_base = aslr.get_base_address(original_base);
        let randomized_heap = aslr.get_heap_address(original_heap);
        let randomized_stack = aslr.get_stack_address(original_stack);

        // Addresses should be different from originals
        assert_ne!(randomized_base, original_base);
        assert_ne!(randomized_heap, original_heap);
        assert_ne!(randomized_stack, original_stack);

        // Should be deterministic for same input
        assert_eq!(randomized_base, aslr.get_base_address(original_base));
        assert_eq!(randomized_heap, aslr.get_heap_address(original_heap));
        assert_eq!(randomized_stack, aslr.get_stack_address(original_stack));
    }
}

#[test]
fn test_aslr_applied_flag() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_aslr().unwrap();

    if let Some(aslr) = ctx.aslr_state_mut() {
        assert!(!aslr.is_applied());

        aslr.mark_applied();
        assert!(aslr.is_applied());
    }
}

#[test]
fn test_capability_attestation_enable() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    assert!(ctx.enable_attestation().is_ok());
    assert!(ctx.attestation_state_mut().is_some());
}

#[test]
fn test_capability_token_creation() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        let token = attestation
            .create_token(
                "file_access".to_string(),
                vec!["read".to_string(), "write".to_string()],
            )
            .unwrap();

        assert_eq!(token.id, "file_access");
        assert_eq!(token.permissions.len(), 2);
        assert!(token.verify_signature());
    }
}

#[test]
fn test_capability_delegation() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        // Create parent capability with full permissions
        attestation
            .create_token(
                "root".to_string(),
                vec![
                    "read".to_string(),
                    "write".to_string(),
                    "execute".to_string(),
                    "delete".to_string(),
                ],
            )
            .unwrap();

        // Delegate subset to child1
        let child1 = attestation
            .delegate(
                "root",
                "child1".to_string(),
                vec!["read".to_string(), "write".to_string()],
            )
            .unwrap();

        assert_eq!(child1.permissions.len(), 2);

        // Delegate further subset to grandchild
        let grandchild = attestation
            .delegate("child1", "grandchild".to_string(), vec!["read".to_string()])
            .unwrap();

        assert_eq!(grandchild.permissions.len(), 1);

        // Verify permissions work
        assert!(attestation.verify("grandchild", "read").is_ok());
        assert!(attestation.verify("grandchild", "write").is_err());
    }
}

#[test]
fn test_capability_delegation_permission_subset() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        attestation
            .create_token("parent".to_string(), vec!["read".to_string()])
            .unwrap();

        // Try to delegate permission not in parent
        let result = attestation.delegate("parent", "child".to_string(), vec!["write".to_string()]);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not in parent capability"));
    }
}

#[test]
fn test_capability_verification() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        attestation
            .create_token(
                "fs_cap".to_string(),
                vec!["read".to_string(), "write".to_string()],
            )
            .unwrap();

        // Verify existing permission
        assert!(attestation.verify("fs_cap", "read").is_ok());
        assert!(attestation.verify("fs_cap", "write").is_ok());

        // Verify non-existing permission
        assert!(attestation.verify("fs_cap", "execute").is_err());
    }
}

#[test]
fn test_capability_revocation() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        attestation
            .create_token("parent".to_string(), vec!["read".to_string()])
            .unwrap();

        attestation
            .delegate("parent", "child".to_string(), vec!["read".to_string()])
            .unwrap();

        // Verify works before revocation
        assert!(attestation.verify("child", "read").is_ok());

        // Revoke parent
        attestation.revoke("parent").unwrap();

        // Both should be revoked
        assert!(attestation.verify("parent", "read").is_err());
        assert!(attestation.verify("child", "read").is_err());
    }
}

#[test]
fn test_capability_revocation_cascading() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    ctx.enable_attestation().unwrap();

    if let Some(attestation) = ctx.attestation_state_mut() {
        // Create hierarchy: root -> child1 -> grandchild1
        //                        -> child2 -> grandchild2
        attestation
            .create_token("root".to_string(), vec!["read".to_string()])
            .unwrap();
        attestation
            .delegate("root", "child1".to_string(), vec!["read".to_string()])
            .unwrap();
        attestation
            .delegate("root", "child2".to_string(), vec!["read".to_string()])
            .unwrap();
        attestation
            .delegate(
                "child1",
                "grandchild1".to_string(),
                vec!["read".to_string()],
            )
            .unwrap();
        attestation
            .delegate(
                "child2",
                "grandchild2".to_string(),
                vec!["read".to_string()],
            )
            .unwrap();

        // Revoke child1
        attestation.revoke("child1").unwrap();

        // child1 and grandchild1 should be revoked
        assert!(attestation.verify("child1", "read").is_err());
        assert!(attestation.verify("grandchild1", "read").is_err());

        // child2 and grandchild2 should still work
        assert!(attestation.verify("child2", "read").is_ok());
        assert!(attestation.verify("grandchild2", "read").is_ok());
    }
}

#[test]
fn test_capability_token_signature_verification() {
    use mielin_wasm::security::CapabilityToken;

    let token = CapabilityToken::new(
        "test_cap".to_string(),
        vec!["read".to_string(), "write".to_string()],
    );

    // Original signature should verify
    assert!(token.verify_signature());

    // Tampered token should not verify
    let mut tampered = token.clone();
    tampered.permissions.push("execute".to_string());
    assert!(!tampered.verify_signature());
}

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
    assert!(!standard.enable_aslr);
    assert!(!standard.enable_attestation);

    let permissive = SecurityPolicy::permissive();
    assert!(!permissive.enable_cfi);
    assert!(!permissive.enable_stack_canaries);
    assert!(!permissive.enable_aslr);
    assert!(!permissive.enable_attestation);
}

#[test]
fn test_security_context_disabled_features() {
    let policy = SecurityPolicy::permissive();
    let mut ctx = SecurityContext::new(policy);

    // Features should fail to enable due to policy
    assert!(ctx.enable_cfi(100).is_err());
    assert!(ctx.enable_stack_canaries().is_err());
    assert!(ctx.enable_aslr().is_err());
    assert!(ctx.enable_attestation().is_err());
}

#[test]
fn test_security_stats_collection() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    // Enable all features
    ctx.enable_cfi(100).unwrap();
    ctx.enable_stack_canaries().unwrap();
    ctx.enable_aslr().unwrap();
    ctx.enable_attestation().unwrap();

    // Generate some activity
    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);
    }

    if let Some(canary) = ctx.canary_state_mut() {
        canary.place_canary(1, 0x1000);
    }

    if let Some(attestation) = ctx.attestation_state_mut() {
        attestation
            .create_token("test".to_string(), vec!["read".to_string()])
            .unwrap();
    }

    // Get stats
    let stats = ctx.stats();
    assert!(stats.cfi_stats.is_some());
    assert!(stats.canary_stats.is_some());
    assert!(stats.aslr_stats.is_some());
    assert!(stats.attestation_stats.is_some());
}

#[test]
fn test_integrated_security_features() {
    let policy = SecurityPolicy::strict();
    let mut ctx = SecurityContext::new(policy);

    // Enable all security features
    ctx.enable_cfi(50).unwrap();
    ctx.enable_stack_canaries().unwrap();
    ctx.enable_aslr().unwrap();
    ctx.enable_attestation().unwrap();

    // Setup CFI
    if let Some(cfi) = ctx.cfi_state_mut() {
        cfi.add_call_target(0x1000);
        cfi.add_call_target(0x2000);
    }

    // Setup canaries
    let canary_value = ctx.canary_state_mut().unwrap().get_canary_value();
    if let Some(canary) = ctx.canary_state_mut() {
        canary.place_canary(1, 0x1000);
    }

    // Setup attestation
    if let Some(attestation) = ctx.attestation_state_mut() {
        attestation
            .create_token("exec".to_string(), vec!["execute".to_string()])
            .unwrap();
    }

    // Validate CFI call
    assert!(ctx.validate_call(0x1000).is_ok());

    // Validate canary
    if let Some(canary) = ctx.canary_state_mut() {
        assert!(canary.verify_canary(1, canary_value).is_ok());
    }

    // Validate capability
    if let Some(attestation) = ctx.attestation_state_mut() {
        assert!(attestation.verify("exec", "execute").is_ok());
    }

    // Get comprehensive stats
    let stats = ctx.stats();
    assert!(stats.cfi_stats.is_some());
    assert!(stats.canary_stats.is_some());
    assert!(stats.aslr_stats.is_some());
    assert!(stats.attestation_stats.is_some());
}
