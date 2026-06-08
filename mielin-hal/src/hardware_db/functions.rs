//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use alloc::string::String;

use super::hardwaredatabase_type::HardwareDatabase;

/// Get a summary of all processors in the database
pub fn database_summary() -> String {
    let db = HardwareDatabase::new();
    let mut summary = String::new();
    summary
        .push_str(
            &alloc::format!("Hardware Database: {} processors\n\n", db.processors.len()),
        );
    let vendors = [
        "Intel",
        "AMD",
        "ARM",
        "AWS",
        "Apple",
        "Qualcomm",
        "MediaTek",
        "Ampere",
        "StarFive",
        "SiFive",
        "T-Head",
        "Loongson",
        "NVIDIA",
        "Samsung",
        "Google",
        "HiSilicon",
        "Rockchip",
        "Espressif",
        "STMicroelectronics",
        "NXP",
        "Raspberry Pi",
    ];
    for vendor in &vendors {
        let procs = db.find_by_vendor(vendor);
        if !procs.is_empty() {
            summary
                .push_str(&alloc::format!("{} ({} processors):\n", vendor, procs.len()));
            for proc in procs {
                summary
                    .push_str(
                        &alloc::format!(
                            "  - {} ({} cores, {}-{} MHz, L3: {} KB)\n", proc.name, proc
                            .physical_cores, proc.base_freq_mhz, proc.max_freq_mhz, proc
                            .l3_total_kb
                        ),
                    );
            }
            summary.push('\n');
        }
    }
    summary
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::HardwareCapabilities;
    use crate::Architecture;
    #[test]
    fn test_database_creation() {
        let db = HardwareDatabase::new();
        assert!(! db.processors.is_empty());
        assert!(
            db.processors.len() >= 50,
            "Database should have at least 50 processors, found {}", db.processors.len()
        );
    }
    #[test]
    fn test_database_count() {
        let db = HardwareDatabase::new();
        let count = db.processors.len();
        let intel_count = db.find_by_vendor("Intel").len();
        let amd_count = db.find_by_vendor("AMD").len();
        let arm_count = db.find_by_vendor("ARM").len();
        let qualcomm_count = db.find_by_vendor("Qualcomm").len();
        let mediatek_count = db.find_by_vendor("MediaTek").len();
        let apple_count = db.find_by_vendor("Apple").len();
        assert!(count >= 50, "Total processors: {}", count);
        assert!(intel_count >= 10, "Intel processors: {}", intel_count);
        assert!(amd_count >= 10, "AMD processors: {}", amd_count);
        assert!(arm_count >= 10, "ARM processors: {}", arm_count);
        assert!(qualcomm_count >= 4, "Qualcomm processors: {}", qualcomm_count);
        assert!(mediatek_count >= 3, "MediaTek processors: {}", mediatek_count);
        assert_eq!(apple_count, 3, "Apple processors: {}", apple_count);
    }
    #[test]
    fn test_find_by_name() {
        let db = HardwareDatabase::new();
        let proc = db.find_by_name("i9-13900K");
        assert!(proc.is_some());
        assert_eq!(proc.expect("proc exists").vendor, "Intel");
    }
    #[test]
    fn test_find_by_vendor() {
        let db = HardwareDatabase::new();
        let intel_procs = db.find_by_vendor("Intel");
        assert!(! intel_procs.is_empty());
        assert!(intel_procs.iter().all(| p | p.vendor == "Intel"));
    }
    #[test]
    fn test_find_by_architecture() {
        let db = HardwareDatabase::new();
        let x86_procs = db.find_by_architecture(Architecture::X86_64);
        assert!(! x86_procs.is_empty());
        assert!(x86_procs.iter().all(| p | p.architecture == Architecture::X86_64));
    }
    #[test]
    fn test_find_with_capabilities() {
        let db = HardwareDatabase::new();
        let avx512_procs = db.find_with_capabilities(HardwareCapabilities::AVX512);
        assert!(! avx512_procs.is_empty());
        for proc in avx512_procs {
            assert!(proc.capabilities.contains(HardwareCapabilities::AVX512));
        }
    }
    #[test]
    fn test_cache_score() {
        let db = HardwareDatabase::new();
        let proc = db.find_by_name("M3 Max").expect("M3 Max exists");
        let score = proc.cache_score();
        assert!(score > 0.0);
    }
    #[test]
    fn test_performance_score() {
        let db = HardwareDatabase::new();
        let proc1 = db.find_by_name("i9-13900K").expect("i9 exists");
        let proc2 = db.find_by_name("Ryzen 7 5800X").expect("Ryzen exists");
        let score1 = proc1.performance_score();
        let score2 = proc2.performance_score();
        assert!(score1 > 0.0);
        assert!(score2 > 0.0);
        assert!(score1 > score2);
    }
    #[test]
    fn test_database_summary() {
        let summary = database_summary();
        assert!(! summary.is_empty());
        assert!(summary.contains("Hardware Database"));
    }
    #[test]
    fn test_all_processors_have_valid_specs() {
        let db = HardwareDatabase::new();
        for proc in db.all_processors() {
            assert!(! proc.name.is_empty());
            assert!(! proc.vendor.is_empty());
            assert!(proc.physical_cores > 0);
            assert!(proc.logical_cores >= proc.physical_cores);
            assert!(proc.max_freq_mhz >= proc.base_freq_mhz);
            assert!(proc.cache_line_bytes > 0);
        }
    }
    #[test]
    fn test_apple_processors() {
        let db = HardwareDatabase::new();
        let apple_procs = db.find_by_vendor("Apple");
        assert_eq!(apple_procs.len(), 3);
        for proc in apple_procs {
            assert_eq!(proc.cache_line_bytes, 128);
            assert!(proc.capabilities.contains(HardwareCapabilities::NEON));
        }
    }
}
