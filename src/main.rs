mod tracer;
mod storage;
mod server;
mod stats;
mod launcher;
mod tui;

use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
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
            let output = args.last().unwrap().clone();
            let prog_args: Vec<String> = args[3..args.len() - 1].to_vec();
            run_and_trace(program, &prog_args, &output);
        }
        "trace" => {
            if args.len() < 4 {
                eprintln!("Usage: {} trace <pid> <output.tdb>", args[0]);
                std::process::exit(1);
            }
            let pid: i32 = args[2].parse().expect("Invalid PID");
            let output = &args[3];
            attach_and_trace(pid, output);
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
        "tui" => {
            if args.len() < 3 {
                eprintln!("Usage: {} tui <trace.tdb>", args[0]);
                std::process::exit(1);
            }
            let trace_file = &args[2];
            if let Err(e) = tui::run(trace_file) {
                eprintln!("TUI error: {}", e);
                std::process::exit(1);
            }
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
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }
}

fn print_usage(prog: &str) {
    eprintln!("TDB - Timeless Debugger for macOS\n");
    eprintln!("Usage: {} <command> [args]\n", prog);
    eprintln!("Commands:");
    eprintln!("  run <program> [args...] <output.tdb>  Run and trace a program");
    eprintln!("  trace <pid> <output.tdb>              Attach to running process");
    eprintln!("  view <trace.tdb> [port]               View trace in browser");
    eprintln!("  tui <trace.tdb>                       View trace in terminal");
    eprintln!("  stats <trace.tdb>                     Show trace statistics");
}

fn run_and_trace(program: &str, args: &[String], output: &str) {
    println!("Launching: {} {}", program, args.join(" "));
    let launcher = launcher::ProcessLauncher::launch(program, args)
        .expect("Failed to launch program");
    println!("Process started with PID: {} (stopped at entry)", launcher.pid);
    trace_loop(launcher.pid, output);
}

fn attach_and_trace(pid: i32, output: &str) {
    println!("Attaching to PID: {}...", pid);
    let _launcher = launcher::ProcessLauncher::attach(pid)
        .expect("Failed to attach (need sudo?)");
    println!("Attached and stopped.");
    trace_loop(pid, output);
}

fn trace_loop(pid: i32, output: &str) {
    println!("\n  TDB - Timeless Debugger\n");

    let mut tracer = tracer::Tracer::new(pid, output)
        .expect("Failed to create tracer");

    // Ctrl+C sets the flag so we can save before exiting
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    println!("  Tracing... Press Ctrl+C to stop\n");

    let mut error_count: u32 = 0;
    let mut call_count: u64 = 0;
    let mut return_count: u64 = 0;
    let mut mem_change_count: u64 = 0;
    let mut last_print_step: u64 = 0;

    loop {
        if !running.load(Ordering::SeqCst) {
            println!("\n  Stopping trace...");
            break;
        }

        match tracer.single_step() {
            tracer::StepResult::Ok(entry) => {
                error_count = 0;

                if entry.insn_text.contains("CALL") {
                    call_count += 1;
                }
                if entry.insn_text.contains("RETURN") {
                    return_count += 1;
                }
                mem_change_count += entry.mem_changes.len() as u64;

                // Print progress: every 1000 steps, or on interesting events
                let interesting = entry.insn_text.contains("CALL")
                    || entry.insn_text.contains("RETURN")
                    || !entry.mem_changes.is_empty();

                if entry.step % 1000 == 0
                    || (entry.step - last_print_step > 100 && interesting)
                {
                    let marker = if entry.insn_text.contains("CALL") {
                        ">"
                    } else if entry.insn_text.contains("RETURN") {
                        "<"
                    } else if !entry.mem_changes.is_empty() {
                        "*"
                    } else {
                        " "
                    };

                    let insn = if entry.insn_text.len() > 50 {
                        format!("{}...", &entry.insn_text[..47])
                    } else {
                        entry.insn_text.clone()
                    };

                    println!(
                        "  {} {:>7}  0x{:012x}  {}",
                        marker, entry.step, entry.pc, insn
                    );
                    last_print_step = entry.step;
                }
            }
            tracer::StepResult::ProcessExited(code) => {
                println!("\n  Process exited with code {}", code);
                break;
            }
            tracer::StepResult::Error(e) => {
                error_count += 1;
                if error_count > 10 {
                    eprintln!("\n  Too many errors, stopping: {}", e);
                    break;
                }
            }
        }
    }

    // Always save, even after Ctrl+C
    println!("\n  Saving trace to {}...", output);
    tracer.db().save().expect("Failed to save trace");
    tracer.detach();

    let total = tracer.step_count();
    println!("\n  Trace Summary");
    println!("  -------------");
    println!("  Total Steps:     {:>10}", total);
    println!("  Function Calls:  {:>10}", call_count);
    println!("  Returns:         {:>10}", return_count);
    println!("  Memory Changes:  {:>10}", mem_change_count);
    println!();

    let exe = env::args().next().unwrap_or_else(|| "tdb".to_string());
    println!("  View the trace:");
    println!("    {} view {}      (browser at http://localhost:8080)", exe, output);
    println!("    {} tui {}       (terminal UI)", exe, output);
    println!();
}

async fn view_trace(trace_file: &str, port: u16) {
    println!("Loading trace from {}...", trace_file);
    let db = Arc::new(
        storage::TraceDb::load(trace_file).expect("Failed to load trace"),
    );
    println!("Loaded {} steps", db.count());

    let stats = stats::TraceStats::analyze(&db);
    println!(
        "  {} unique addrs, {} calls, {} returns, {} mem changes",
        stats.unique_addresses, stats.call_count, stats.ret_count, stats.mem_change_count
    );

    server::serve(db, port).await;
}

fn show_stats(trace_file: &str) {
    println!("Loading trace from {}...\n", trace_file);
    let db = storage::TraceDb::load(trace_file).expect("Failed to load trace");
    let stats = stats::TraceStats::analyze(&db);
    stats.print();
}
