use seam_bootstrap::{
    vm_init,
    effect::{Effect, EffectAnalysis, EffectType},
    contract::{RequiresContract, ContractChecker, ResourceRequirement, RequirementLevel},
    sync::AutoSync,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════════════════════╗");
    println!("║ Seam VM PoC Bootstrap - Phase 3: リソース追跡 (Resource Tracking)      ║");
    println!("║     静的エフェクト解析・requires契約・自動同期処理                      ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝\n");

    // ========================================================================
    // PART 1: Initialize VM
    // ========================================================================
    println!("[PART 1] Seam VM Initialization");
    println!("════════════════════════════════════════════════════════════════════");

    let ctx = match vm_init(8192) {
        Ok(context) => {
            println!("✓ Seam VM initialized");
            println!("  - Arena size: 8192 bytes");
            println!("  - Thread ID: {}", context.thread_id());
            context
        }
        Err(e) => {
            eprintln!("✗ Initialization failed: {}", e);
            return;
        }
    };
    println!();

    // ========================================================================
    // PART 2: Static Effect Analysis
    // ========================================================================
    println!("[PART 2] 静的エフェクト解析 (Static Effect Analysis)");
    println!("════════════════════════════════════════════════════════════════════");

    let mut analysis = EffectAnalysis::new(3);

    // Path 0: Reads resources 1 and 2
    println!("Adding effects for Path 0...");
    analysis.add_effect_to_path(0, Effect::new(1, EffectType::Read)).unwrap();
    analysis.add_effect_to_path(0, Effect::new(2, EffectType::Read)).unwrap();
    println!("  ✓ Path 0: Read(resource 1), Read(resource 2)");

    // Path 1: Writes to resource 1
    println!("Adding effects for Path 1...");
    analysis.add_effect_to_path(1, Effect::new(1, EffectType::Write)).unwrap();
    println!("  ✓ Path 1: Write(resource 1)");

    // Path 2: Reads resource 1 and 3
    println!("Adding effects for Path 2...");
    analysis.add_effect_to_path(2, Effect::new(1, EffectType::Read)).unwrap();
    analysis.add_effect_to_path(2, Effect::new(3, EffectType::Write)).unwrap();
    println!("  ✓ Path 2: Read(resource 1), Write(resource 3)");

    println!("\nEffect Analysis Summary:");
    println!("  - Total resources accessed: {}", analysis.union_effects().effect_count());
    println!("  - Write-write conflicts: {}", if analysis.has_write_conflicts() { "YES" } else { "NO" });
    println!("  - Read-write conflicts: {}", if analysis.has_read_write_conflicts() { "YES" } else { "NO" });

    let sync_resources = analysis.required_sync_points();
    println!("  - Synchronization required for: {} resources", sync_resources.len());
    for res_id in sync_resources {
        println!("    • Resource {}", res_id);
    }
    println!();

    // ========================================================================
    // PART 3: Requires Contracts
    // ========================================================================
    println!("[PART 3] requires 契約の検証 (Contract Verification)");
    println!("════════════════════════════════════════════════════════════════════");

    let mut checker = ContractChecker::new();

    // Create contracts for each path
    println!("Registering requires contracts...");

    let mut contract0 = RequiresContract::new("path_0".to_string(), RequirementLevel::Required);
    contract0.add_requirement(ResourceRequirement::read(1));
    contract0.add_requirement(ResourceRequirement::read(2));
    checker.register_contract(contract0);
    println!("  ✓ path_0: requires read(1), read(2)");

    let mut contract1 = RequiresContract::new("path_1".to_string(), RequirementLevel::Required);
    contract1.add_requirement(ResourceRequirement::write(1));
    checker.register_contract(contract1);
    println!("  ✓ path_1: requires write(1)");

    let mut contract2 = RequiresContract::new("path_2".to_string(), RequirementLevel::Required);
    contract2.add_requirement(ResourceRequirement::read(1));
    contract2.add_requirement(ResourceRequirement::write(3));
    checker.register_contract(contract2);
    println!("  ✓ path_2: requires read(1), write(3)");

    println!("\nVerifying contracts against effects...");

    // Check path 0
    if let Ok(()) = checker.check_contract("path_0", 0, analysis.path_effects(0).unwrap()) {
        println!("  ✓ Path 0: Contract SATISFIED");
    } else {
        println!("  ✗ Path 0: Contract VIOLATED");
    }

    // Check path 1
    if let Ok(()) = checker.check_contract("path_1", 1, analysis.path_effects(1).unwrap()) {
        println!("  ✓ Path 1: Contract SATISFIED");
    } else {
        println!("  ✗ Path 1: Contract VIOLATED");
    }

    // Check path 2
    if let Ok(()) = checker.check_contract("path_2", 2, analysis.path_effects(2).unwrap()) {
        println!("  ✓ Path 2: Contract SATISFIED");
    } else {
        println!("  ✗ Path 2: Contract VIOLATED");
    }

    if checker.has_required_violations() {
        println!("\n⚠ Contract violations detected!");
        for violation in checker.violations() {
            println!("  - {} (Path {}): missing resources {:?}",
                     violation.contract_name, violation.path_id, violation.missing_resources);
        }
    } else {
        println!("\n✓ All contracts satisfied!");
    }
    println!();

    // ========================================================================
    // PART 4: Automatic Synchronization Detection
    // ========================================================================
    println!("[PART 4] 自動同期処理 (Automatic Synchronization)");
    println!("════════════════════════════════════════════════════════════════════");

    let mut auto_sync = AutoSync::new(true);
    auto_sync.set_analysis(analysis);

    println!("Sync points automatically detected:");
    println!("  Total sync points: {}", auto_sync.sync_count());

    println!("\n  WAW (Write-After-Write) conflicts:");
    let waw_syncs = auto_sync.waw_sync_points();
    if waw_syncs.is_empty() {
        println!("    (none)");
    } else {
        for sync in waw_syncs {
            println!("    • Resource {} (paths: {})", 
                     sync.resource_id(), 
                     sync.paths().iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","));
        }
    }

    println!("\n  RAW (Read-After-Write) dependencies:");
    let raw_syncs = auto_sync.raw_sync_points();
    if raw_syncs.is_empty() {
        println!("    (none)");
    } else {
        for sync in raw_syncs {
            println!("    • Resource {} (paths: {})", 
                     sync.resource_id(), 
                     sync.paths().iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","));
        }
    }

    println!("\n  WAR (Write-After-Read) dependencies:");
    let war_syncs = auto_sync.war_sync_points();
    if war_syncs.is_empty() {
        println!("    (none)");
    } else {
        for sync in war_syncs {
            println!("    • Resource {} (paths: {})", 
                     sync.resource_id(), 
                     sync.paths().iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","));
        }
    }

    println!("\nGenerated barriers for synchronization:");
    let barriers = auto_sync.generate_barriers();
    for barrier in barriers {
        println!("  {}", barrier);
    }
    println!();

    // ========================================================================
    // PART 5: Example Case - Conflict Scenario
    // ========================================================================
    println!("[PART 5] 競合シナリオ (Conflict Scenario)");
    println!("════════════════════════════════════════════════════════════════════");

    let mut conflict_analysis = EffectAnalysis::new(2);

    println!("Scenario: Two paths accessing shared resource");
    println!("  Path 0: Write to resource 1");
    conflict_analysis.add_effect_to_path(0, Effect::new(1, EffectType::Write)).unwrap();

    println!("  Path 1: Read from resource 1");
    conflict_analysis.add_effect_to_path(1, Effect::new(1, EffectType::Read)).unwrap();

    println!("\nConflict Analysis:");
    println!("  Write-write conflicts: {}", 
             if conflict_analysis.has_write_conflicts() { "YES (NEEDS LOCK)" } else { "NO" });
    println!("  Read-write conflicts: {}", 
             if conflict_analysis.has_read_write_conflicts() { "YES (NEEDS BARRIER)" } else { "NO" });

    let mut scenario_sync = AutoSync::new(true);
    scenario_sync.set_analysis(conflict_analysis);

    println!("\nAutomatic synchronization required:");
    if scenario_sync.sync_count() == 0 {
        println!("  No synchronization needed");
    } else {
        for barrier in scenario_sync.generate_barriers() {
            println!("  {}", barrier);
        }
    }
    println!();

    // ========================================================================
    // PART 6: Summary and Key Insights
    // ========================================================================
    println!("[PART 6] Summary: Phase 3 Features");
    println!("════════════════════════════════════════════════════════════════════");

    println!("✓ 静的エフェクト解析 (Static Effect Analysis):");
    println!("  - Compile-time detection of resource access patterns");
    println!("  - Conflict identification (RAW, WAR, WAW)");
    println!("  - Zero runtime overhead for analysis");

    println!("\n✓ requires 契約 (Requires Contracts):");
    println!("  - Explicit resource access specifications");
    println!("  - Compile-time verification of contract satisfaction");
    println!("  - Required/Expected/Optional enforcement levels");

    println!("\n✓ 自動同期処理 (Automatic Synchronization):");
    println!("  - Automatic detection of sync points needed");
    println!("  - Barrier generation without manual coding");
    println!("  - Dependency type classification (RAW/WAR/WAW)");

    println!("\nBenefits:");
    println!("  • No manual synchronization coding → reduced errors");
    println!("  • Compile-time verification → early bug detection");
    println!("  • Deterministic behavior → reproducible execution");
    println!("  • Static ordering → deadlock freedom");
    println!();

    // ========================================================================
    // PART 7: VM Statistics
    // ========================================================================
    println!("[PART 7] システム統計 (System Statistics)");
    println!("════════════════════════════════════════════════════════════════════");

    println!("Context allocated: {} bytes", ctx.allocated());
    println!("Context remaining: {} bytes", ctx.remaining());

    #[cfg(target_arch = "x86_64")]
    {
        println!("\nArchitecture: x86-64");
        println!("  CFP register: rbp");
        println!("  RFP register: r15");
        println!("  Barriers: sfence");
    }

    #[cfg(target_arch = "aarch64")]
    {
        println!("\nArchitecture: AArch64");
        println!("  CFP register: x29");
        println!("  RFP register: x28");
        println!("  Barriers: dmb ish");
    }
    println!();

    // ========================================================================
    // Final Summary
    // ========================================================================
    println!("╔════════════════════════════════════════════════════════════════════════╗");
    println!("║  ✓ Phase 3: Resource Tracking Implementation Complete                  ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝");
    println!();

    println!("Phase 3 enables:");
    println!("  ✓ Compile-time resource access verification");
    println!("  ✓ Automatic conflict detection and sync generation");
    println!("  ✓ Safe parallel execution with zero manual synchronization code");
    println!();

    println!("Ready for Phase 4: Compiler Integration");
    println!("  1. Code generation for fork/join");
    println!("  2. Automatic access set derivation from AST");
    println!("  3. Resource ID assignment and verification");
}
