use seam_bootstrap::{
    vm_init,
    compiler::SeamCompiler,
    codegen::CodeGenerator,
    seam_parser::parse_seam,
    type_checker::check_seam,
};

fn main() {
    println!("╔════════════════════════════════════════════════════════════════════════╗");
    println!("║ Seam VM PoC Bootstrap - Phase 4: コンパイラ統合 (Compiler Integration)  ║");
    println!("║        AST・コンパイル・コード生成                                      ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝\n");

    // ========================================================================
    // PART 1: Initialize VM and Compiler
    // ========================================================================
    println!("[PART 1] Initialization");
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

    let mut compiler = SeamCompiler::new();
    println!("✓ Seam Compiler initialized");
    println!();

    // ========================================================================
    // PART 2: Define Fork Source Code
    // ========================================================================
    println!("[PART 2] Fork Source Definition");
    println!("════════════════════════════════════════════════════════════════════");

    let fork_source = r#"
        fork(1) {
            path(0) { accesses: read(1), read(2); code: process_path_0() }
            path(1) { accesses: write(1); code: process_path_1() }
            path(2) { accesses: read(1), write(3); code: process_path_2() }
        }
    "#;

    println!("Source Code:");
    println!("{}", fork_source.trim());
    println!();

    // ========================================================================
    // PART 3: Parse Fork Expression
    // ========================================================================
    println!("[PART 3] Parse: Source → AST");
    println!("════════════════════════════════════════════════════════════════════");

    let fork_expr = match compiler.parse_fork(fork_source) {
        Ok(expr) => {
            println!("✓ Parsing successful");
            println!("  - Fork ID: {}", expr.fork_id);
            println!("  - Paths: {}", expr.path_count());

            let resources = expr.resources_accessed();
            println!("  - Unique resources: {}", resources.len());
            for res in resources {
                println!("    • Resource {}", res.0);
            }

            println!("\n✓ AST Structure:");
            for path in &expr.paths {
                print!("  Path {} requires: ", path.path_id);
                let accesses = path
                    .requires
                    .accesses
                    .iter()
                    .map(|a| {
                        format!(
                            "{}({})",
                            if a.access_type == seam_bootstrap::AccessType::Read {
                                "R"
                            } else {
                                "W"
                            },
                            a.resource_id.0
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("{}", accesses);
            }

            expr
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
            return;
        }
    };
    println!();

    // ========================================================================
    // PART 4: Compile to Intermediate Representation
    // ========================================================================
    println!("[PART 4] Compile: AST → Intermediate Representation");
    println!("════════════════════════════════════════════════════════════════════");

    let compiled = match compiler.compile(&fork_expr) {
        Ok(compiled) => {
            println!("✓ Compilation successful");
            println!("  - Fork ID: {}", compiled.fork_id);
            println!("  - Number of paths: {}", compiled.num_paths);
            println!("  - Unique resources: {}", compiled.unique_resources().len());

            println!("\n✓ Compiled Structure:");
            for (res_id, accesses) in &compiled.resource_map {
                print!("  Resource {}: ", res_id.0);
                let access_strs = accesses
                    .iter()
                    .map(|a| match a {
                        seam_bootstrap::AccessType::Read => "READ",
                        seam_bootstrap::AccessType::Write => "WRITE",
                        seam_bootstrap::AccessType::ReadWrite => "R/W",
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("{}", access_strs);
            }

            compiled
        }
        Err(e) => {
            eprintln!("✗ Compilation error: {}", e);
            return;
        }
    };
    println!();

    // ========================================================================
    // PART 5: Static Analysis
    // ========================================================================
    println!("[PART 5] Analyze: Static Effect & Contract Analysis");
    println!("════════════════════════════════════════════════════════════════════");

    let analysis = match compiler.analyze(&compiled) {
        Ok(mut analysis) => {
            println!("✓ Analysis completed");

            println!("\n  Conflict Detection:");
            println!(
                "    - Write-write conflicts: {}",
                if analysis.has_write_conflicts {
                    "YES (needs mutex)"
                } else {
                    "NO"
                }
            );
            println!(
                "    - Read-write conflicts: {}",
                if analysis.has_read_write_conflicts {
                    "YES (needs barrier)"
                } else {
                    "NO"
                }
            );

            println!("\n  Synchronization Resources:");
            for res_id in &analysis.sync_resources {
                println!("    • Resource {}", res_id);
            }

            println!("\n  Contract Verification:");
            for path_id in 0..compiled.num_paths {
                let contract = format!("path_{}", path_id);
                let effects = analysis.effect_analysis.path_effects(path_id as u32).unwrap();
                match analysis
                    .contract_checker
                    .check_contract(&contract, path_id as u32, effects)
                {
                    Ok(()) => println!("    ✓ Path {}: Contract satisfied", path_id),
                    Err(_) => println!("    ✗ Path {}: Contract violated", path_id),
                }
            }

            println!("\n  Auto-Sync Detection:");
            println!("    - Total sync points: {}", analysis.auto_sync.sync_count());

            let barriers = analysis.auto_sync.generate_barriers();
            for barrier in barriers {
                println!("    • {}", barrier);
            }

            analysis
        }
        Err(e) => {
            eprintln!("✗ Analysis error: {}", e);
            return;
        }
    };
    println!();

    // ========================================================================
    // PART 6: Generate Code
    // ========================================================================
    println!("[PART 6] Generate: IR → Executable Code");
    println!("════════════════════════════════════════════════════════════════════");

    let generated = CodeGenerator::generate(&compiled, &analysis);

    println!("✓ Code generation completed");
    println!("\n  Fork Setup:");
    for line in generated.fork_setup.lines().take(3) {
        println!("    {}", line);
    }
    println!("    ...");

    println!("\n  Path Executions: {} paths", generated.path_executions.len());
    for (i, path_code) in generated.path_executions.iter().enumerate() {
        println!("    Path {} code block:", i);
        for line in path_code.lines().take(2) {
            println!("      {}", line);
        }
    }

    println!("\n  Synchronization:");
    for line in generated.synchronization.lines().take(3) {
        println!("    {}", line);
    }

    println!("\n  Join Handling:");
    for line in generated.join_handling.lines().take(3) {
        println!("    {}", line);
    }
    println!();

    // ========================================================================
    // PART 7: Generate Pseudo-Code
    // ========================================================================
    println!("[PART 7] Pseudo-Code Output");
    println!("════════════════════════════════════════════════════════════════════");

    let pseudocode = CodeGenerator::generate_pseudocode(&compiled, &analysis);
    println!("{}", pseudocode);

    // ========================================================================
    // PART 8: Generate Resource Map
    // ========================================================================
    println!("[PART 8] Resource Access Map");
    println!("════════════════════════════════════════════════════════════════════");

    let resource_map = CodeGenerator::generate_resource_map(&compiled);
    println!("{}", resource_map);

    // ========================================================================
    // PART 9: Compilation Summary
    // ========================================================================
    println!("[PART 9] Compilation Summary");
    println!("════════════════════════════════════════════════════════════════════");

    println!("✓ Full Compilation Pipeline Completed:");
    println!("  1. Parse: Source code → AST");
    println!("     • {} paths parsed", compiled.num_paths);
    println!("     • {} unique resources identified", compiled.unique_resources().len());

    println!("\n  2. Analyze: AST → Static effects");
    println!(
        "     • Read-write conflicts: {}",
        if analysis.has_read_write_conflicts {
            "YES"
        } else {
            "NO"
        }
    );
    println!(
        "     • {} sync points required",
        analysis.auto_sync.sync_count()
    );

    println!("\n  3. Generate: Effects → Executable code");
    println!("     • Fork setup code generated");
    println!("     • {} path execution blocks", generated.path_executions.len());
    println!("     • Synchronization barriers inserted");
    println!("     • Join handling code generated");

    println!();

    // ========================================================================
    // PART 10: Phase 4 Features and Benefits
    // ========================================================================
    println!("[PART 10] Phase 4 Features & Benefits");
    println!("════════════════════════════════════════════════════════════════════");

    println!("✓ AST (Abstract Syntax Tree):");
    println!("  - Source-code representation");
    println!("  - Fork/path/access specifications");
    println!("  - Contract declarations");

    println!("\n✓ Compiler Pipeline:");
    println!("  - Source parsing with error recovery");
    println!("  - AST validation and normalization");
    println!("  - Static effect extraction");
    println!("  - Conflict detection (RAW/WAR/WAW)");

    println!("\n✓ Code Generation:");
    println!("  - Automatic fork/join code");
    println!("  - Barrier insertion from analysis");
    println!("  - Resource ID assignment");
    println!("  - Pseudo-code output for verification");

    println!("\n✓ End-to-End Compilation:");
    println!("  • Source code → Executable with zero manual sync code");
    println!("  • Compile-time verification of all resources");
    println!("  • Deterministic barrier placement");
    println!("  • Ready for execution on Phase 1+2 VM");

    println!();

    // ========================================================================
    // PART 11: Architecture Info
    // ========================================================================
    println!("[PART 11] System Architecture");
    println!("════════════════════════════════════════════════════════════════════");

    println!("Memory:");
    println!("  - PSSA Arena: {} bytes used, {} remaining", ctx.allocated(), ctx.remaining());

    #[cfg(target_arch = "x86_64")]
    {
        println!("\nTarget: x86-64");
        println!("  - Control Frame Pointer (CFP): rbp");
        println!("  - Resource Frame Pointer (RFP): r15");
        println!("  - Arena Pointer: r14");
        println!("  - Memory barrier: sfence");
    }

    #[cfg(target_arch = "aarch64")]
    {
        println!("\nTarget: AArch64");
        println!("  - Control Frame Pointer (CFP): x29");
        println!("  - Resource Frame Pointer (RFP): x28");
        println!("  - Arena Pointer: x27");
        println!("  - Memory barrier: dmb ish");
    }

    println!();

    // ========================================================================
    // Final Summary
    // ========================================================================
    println!("╔════════════════════════════════════════════════════════════════════════╗");
    println!("║         ✓ Phase 4: Compiler Integration Complete                       ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝");
    println!();

    // ========================================================================
    // PART 12: Phase 5 - Seam Language Parser
    // ========================================================================
    println!("[PART 12] Phase 5: Seam Language Parser");
    println!("════════════════════════════════════════════════════════════════════");

    let seam_source = r#"
        // Data model (immutable)
        record Data {
            int num;
            string label;
        }

        // Shared mutable state
        resource SharedState {
            var int counter;
            var string status;
        }

        // Leaf channel: processes data
        channel Processor {
            void entry(int value) {
                return;
            }
            void collector {
                return;
            }
        }

        // Recovery channel for collect binding
        channel Recovery {
            void entry() {
                return;
            }
            void collector {
                return;
            }
        }

        // Top-level channel with requires contract and fork
        channel Coordinator {
            requires {
                read  { SharedState.counter; }
                write { SharedState.status;  }
            }
            void entry(int taskCount) {
                if (taskCount) {
                    Processor(42) :collect Recovery;
                    fork {
                        path(0) {
                            requires { read  { SharedState.counter; } }
                            return;
                        }
                        path(1) {
                            requires { write { SharedState.status;  } }
                            return;
                        }
                    }
                    return;
                } else {
                    abort;
                }
            }
            void collector {
                return;
            }
        }
    "#;

    match parse_seam(seam_source) {
        Ok(program) => {
            println!("✓ Parse successful");
            println!("  Items parsed: {}", program.item_count());
            println!("  Records:    {}", program.records().count());
            println!("  Resources:  {}", program.resources().count());
            println!("  Channels:   {}", program.channels().count());
            for ch in program.channels() {
                let req_summary = ch.requires.as_ref().map(|r| {
                    format!("{} reads, {} writes", r.reads.len(), r.writes.len())
                }).unwrap_or_else(|| "none".to_string());
                println!("    channel '{}': requires=[{}], params={}",
                         ch.name, req_summary, ch.entry.params.len());
            }

            // ================================================================
            // PART 13: Phase 5 - Type Checker
            // ================================================================
            println!();
            println!("[PART 13] Phase 5: Type Checker");
            println!("════════════════════════════════════════════════════════════════════");

            let type_result = check_seam(&program);

            if type_result.is_ok() {
                println!("✓ Type check passed — {} channels, {} resources verified",
                         type_result.channel_count, type_result.resource_count);
            } else {
                println!("✗ Type check failed: {} error(s)", type_result.error_count());
                for err in &type_result.errors {
                    println!("    Error: {}", err);
                }
            }

            if !type_result.warnings.is_empty() {
                println!("  Warnings ({}):", type_result.warnings.len());
                for w in &type_result.warnings {
                    println!("    ⚠ {}", w);
                }
            }

            if !type_result.read_write_conflicts.is_empty() {
                println!("  Fork RAW/WAR Conflicts (barrier required):");
                for (wp, rp, res) in &type_result.read_write_conflicts {
                    println!("    path {} ↔ path {}: resource '{}'", wp, rp, res);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Parse error: {}", e);
        }
    }

    println!();

    // ========================================================================
    // Final Summary
    // ========================================================================
    println!("╔════════════════════════════════════════════════════════════════════════╗");
    println!("║         ✓ Phase 5: Language Integration Complete                       ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝");
    println!();

    println!("Seam VM PoC now includes:");
    println!("  ✓ Phase 1: Core VM (PSSA, context, abort, channels)");
    println!("  ✓ Phase 2: 2PST (transactions, resources, fork/join)");
    println!("  ✓ Phase 3: Resource Tracking (effects, contracts, sync)");
    println!("  ✓ Phase 4: Compiler (AST, parsing, analysis, codegen)");
    println!("  ✓ Phase 5: Language (DRAFT syntax, parser, type checker)");
    println!();

    println!("Full pipeline: DRAFT syntax → Tokens → AST → Type Check → IR → Code");
}
