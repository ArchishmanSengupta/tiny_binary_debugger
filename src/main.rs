mod tracer;
mod storage;
mod server;
mod stats;
mod launcher;

use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args]", args[0]);
        eprintln!("Commands:");
        eprintln!("  run <program> [args...] <output.tdb>  - Run and trace a program");
        eprintln!("  trace <pid> <output.tdb>              - Trace a running process");
        eprintln!("  view <trace.tdb> [port]               - View trace in browser");
        eprintln!("  stats <trace.tdb>                     - Show trace statistics");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "run" => {
            if args.len() < 4 {
                eprintln!("Usage: {} run <program> [args...] <output.tdb>", args[0]);
                eprintln!("Example: {} run python3 script.py trace.tdb", args[0]);
                std::process::exit(1);
            }
            let program = &args[2];
            let output = args[args.len() - 1].clone();
            let prog_args: Vec<String> = args[3..args.len()-1].iter().map(|s| s.to_string()).collect();
            run_and_trace(program, &prog_args, &output);
        }
        "trace" => {
            if args.len() < 4 {
                eprintln!("Usage: {} trace <pid> <output.tdb>", args[0]);
                std::process::exit(1);
            }
            let pid: i32 = args[2].parse().expect("Invalid PID");
            let output = &args[3];
            trace_process(pid, output, None);
        }
        "view" => {
            if args.len() < 3 {
                eprintln!("Usage: {} view <trace.tdb> [port]", args[0]);
                std::process::exit(1);
            }
            let trace_file = &args[2];
            let port: u16 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8080);
            view_trace(trace_file, port).await;
        }
        "stats" => {
            if args.len() < 3 {
                eprintln!("Usage: {} stats <trace.tdb>", args[0]);
                std::process::exit(1);
            }
            let trace_file = &args[2];
            show_stats(trace_file);
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}

fn run_and_trace(program: &str, args: &[String], output: &str) {
    println!("Launching: {} {}", program, args.join(" "));
    let mut launcher = launcher::ProcessLauncher::launch(program, args)
        .expect("Failed to launch program");
    
    println!("Process started with PID: {} (paused)", launcher.pid);
    
    trace_process(launcher.pid, output, Some(&mut launcher));
}

fn trace_process(pid: i32, output: &str, mut launcher: Option<&mut launcher::ProcessLauncher>) {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║           TDB - Timeless Debugger for macOS                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    
    println!("📍 Attaching to process PID: {}", pid);
    
    let mut tracer = tracer::Tracer::new(pid, output)
        .expect("Failed to create tracer");

    if let Some(l) = launcher.as_mut() {
        l.resume().expect("Failed to resume process");
        println!("✅ Tracer attached, process resumed\n");
    }

    let auto_stop = launcher.is_some();
    
    if !auto_stop {
        println!("🔍 Tracing... Press Ctrl+C to stop\n");
        ctrlc::set_handler(move || {
            println!("\n⏸️  Stopping trace...");
            std::process::exit(0);
        }).expect("Error setting Ctrl+C handler");
    } else {
        println!("🔍 Tracing until program exits...\n");
        println!("╭─────────┬──────────────────┬────────────────────────────────────╮");
        println!("│ Step    │ Address          │ Instruction                        │");
        println!("├─────────┼──────────────────┼────────────────────────────────────┤");
    }

    let mut error_count = 0;
    let mut call_count = 0;
    let mut return_count = 0;
    let mut mem_change_count = 0;
    let mut last_print_step = 0;
    
    loop {
        if auto_stop {
            if let Some(ref l) = launcher {
                if !l.is_running() {
                    println!("╰─────────┴──────────────────┴────────────────────────────────────╯");
                    println!("\n✅ Process exited normally");
                    break;
                }
            }
        }

        match tracer.single_step() {
            Ok(entry) => {
                error_count = 0;
                
                if entry.insn_text.contains("CALL") || entry.insn_text.contains("bl ") || entry.insn_text.contains("blr") {
                    call_count += 1;
                }
                if entry.insn_text.contains("RETURN") || entry.insn_text.contains("ret") {
                    return_count += 1;
                }
                if !entry.mem_changes.is_empty() {
                    mem_change_count += entry.mem_changes.len();
                }
                
                let is_call = entry.insn_text.contains("CALL") || entry.insn_text.contains("bl ");
                let is_return = entry.insn_text.contains("RETURN") || entry.insn_text.contains("ret");
                let has_mem_change = !entry.mem_changes.is_empty();
                
                if entry.step % 1000 == 0 || (entry.step - last_print_step > 100 && (is_call || is_return || has_mem_change)) {
                    let mut marker = "  ";
                    if is_call { marker = "→ "; }
                    if is_return { marker = "← "; }
                    if has_mem_change { marker = "💾"; }
                    
                    let insn_display = if entry.insn_text.len() > 30 {
                        format!("{}...", &entry.insn_text[..27])
                    } else {
                        entry.insn_text.clone()
                    };
                    
                    println!("│ {:7} │ 0x{:014x} │ {}{:32} │", 
                        entry.step, entry.pc, marker, insn_display);
                    last_print_step = entry.step;
                }
            }
            Err(e) => {
                error_count += 1;
                if error_count > 10 {
                    println!("╰─────────┴──────────────────┴────────────────────────────────────╯");
                    eprintln!("\n⚠️  Too many errors, stopping: {}", e);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    println!("\n💾 Saving trace to {}...", output);
    tracer.db().save().expect("Failed to save trace");
    
    let total_steps = tracer.db().count();
    
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    TRACE SUMMARY                             ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Total Steps:          {:>10}                          ║", total_steps);
    println!("║ Function Calls:       {:>10}                          ║", call_count);
    println!("║ Returns:              {:>10}                          ║", return_count);
    println!("║ Memory Changes:       {:>10}                          ║", mem_change_count);
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    
    println!("✨ View the trace in your browser:");
    println!("   {} view {}", std::env::args().next().unwrap(), output);
    println!("   Then open: http://localhost:8080\n");
}

async fn view_trace(trace_file: &str, port: u16) {
    println!("Loading trace from {}...", trace_file);
    let db = Arc::new(storage::TraceDb::load(trace_file)
        .expect("Failed to load trace"));
    println!("Loaded {} steps", db.count());
    
    let stats = stats::TraceStats::analyze(&db);
    println!("\n{} unique addresses, {} calls, {} returns", 
             stats.unique_addresses, stats.call_count, stats.ret_count);
    
    server::serve(db, port).await;
}

fn show_stats(trace_file: &str) {
    println!("Loading trace from {}...", trace_file);
    let db = storage::TraceDb::load(trace_file)
        .expect("Failed to load trace");
    println!("Analyzing...\n");
    
    let stats = stats::TraceStats::analyze(&db);
    stats.print();
}
