//! Comprehensive benchmark suite for the Grift Lisp interpreter.
//!
//! Covers eval dispatch, arena allocation, GC pressure, operatives,
//! applicative builtins, standard library, composite workloads, and parsing.
//!
//! ```sh
//! cargo run -p grift --example bench --release
//! cargo run -p grift --example bench --release -- --filter eval --json
//! cargo run -p grift --example bench --release -- --save baseline.json
//! cargo run -p grift --example bench --release -- --baseline baseline.json
//! ```

use grift::{Lisp, Value};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

const ARENA_SIZE: usize = 500_000;
const REGRESSION_THRESHOLD_PCT: f64 = 5.0;
// Deep recursion benchmarks (fib, ackermann, mergesort) need a large stack.
const STACK_SIZE: usize = 64 * 1024 * 1024;

// ── CLI Config ───────────────────────────────────────────────────────────────

struct Config {
    filter: Option<String>,
    json: bool,
    baseline: Option<String>,
    save: Option<String>,
    iterations: usize,
    warmup: usize,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config {
        filter: None,
        json: false,
        baseline: None,
        save: None,
        iterations: 1,
        warmup: 0,
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--filter" => {
                i += 1;
                cfg.filter = Some(args[i].clone());
            }
            "--json" => cfg.json = true,
            "--baseline" => {
                i += 1;
                cfg.baseline = Some(args[i].clone());
            }
            "--save" => {
                i += 1;
                cfg.save = Some(args[i].clone());
            }
            "--iterations" => {
                i += 1;
                cfg.iterations = args[i]
                    .parse()
                    .expect("Failed to parse --iterations: expected a positive integer");
            }
            "--warmup" => {
                i += 1;
                cfg.warmup = args[i]
                    .parse()
                    .expect("Failed to parse --warmup: expected a positive integer");
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

// ── Benchmark Descriptor ─────────────────────────────────────────────────────

struct BenchDesc {
    name: &'static str,
    category: &'static str,
    code: &'static str,
    expected: Option<Value>,
}

// ── Benchmark Result ─────────────────────────────────────────────────────────

struct BenchResult {
    name: String,
    category: String,
    median_ns: u128,
    throughput_ops_per_sec: f64,
    ok: bool,
}

// ── Core bench runner ────────────────────────────────────────────────────────

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}

fn run_single(desc: &BenchDesc, warmup: usize, iterations: usize) -> BenchResult {
    let total_iters = warmup + iterations;
    let mut timings = Vec::with_capacity(iterations);
    let mut last_ok = true;

    for i in 0..total_iters {
        let lisp: Lisp<ARENA_SIZE> = Lisp::new();
        let start = Instant::now();
        let result = lisp.eval(desc.code);
        let elapsed = start.elapsed();

        if i >= warmup {
            timings.push(elapsed);
        }

        match &result {
            Ok(val) => {
                if let Some(ref exp) = desc.expected {
                    if !values_equal(val, exp) {
                        eprintln!(
                            "  MISMATCH {}: got {:?}, expected {:?}",
                            desc.name, val, exp
                        );
                        last_ok = false;
                    }
                }
            }
            Err(e) => {
                eprintln!("  ERROR {}: {:?}", desc.name, e);
                last_ok = false;
            }
        }
    }

    timings.sort();
    let median = if timings.is_empty() {
        Duration::ZERO
    } else {
        timings[timings.len() / 2]
    };
    let median_ns = median.as_nanos();
    let throughput = if median_ns > 0 {
        1_000_000_000.0 / median_ns as f64
    } else {
        0.0
    };

    BenchResult {
        name: desc.name.to_string(),
        category: desc.category.to_string(),
        median_ns,
        throughput_ops_per_sec: throughput,
        ok: last_ok,
    }
}

// ── JSON helpers (hand-rolled, no serde) ─────────────────────────────────────

fn escape_json(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn results_to_json(results: &[BenchResult]) -> String {
    let commit = std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unknown".to_string());
    let timestamp = std::env::var("BENCH_TIMESTAMP").unwrap_or_else(|_| {
        // Use SystemTime for a rough ISO-8601 timestamp
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        format!("{}s-since-epoch", dur.as_secs())
    });

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"timestamp\": \"{}\",\n",
        escape_json(&timestamp)
    ));
    json.push_str(&format!(
        "  \"commit\": \"{}\",\n",
        escape_json(&commit)
    ));
    json.push_str("  \"benchmarks\": [\n");

    for (i, r) in results.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"name\": \"{}\",\n",
            escape_json(&r.name)
        ));
        json.push_str(&format!(
            "      \"category\": \"{}\",\n",
            escape_json(&r.category)
        ));
        json.push_str(&format!("      \"median_ns\": {},\n", r.median_ns));
        json.push_str(&format!(
            "      \"throughput_ops_per_sec\": {:.1},\n",
            r.throughput_ops_per_sec
        ));
        json.push_str(&format!(
            "      \"ok\": {}\n",
            if r.ok { "true" } else { "false" }
        ));
        json.push_str("    }");
        if i + 1 < results.len() {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ]\n");
    json.push_str("}\n");
    json
}

fn parse_baseline(path: &str) -> Vec<(String, u128)> {
    let mut file = std::fs::File::open(path).expect("failed to open baseline file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("failed to read baseline file");

    // Minimal JSON parser: extract name/median_ns pairs
    let mut entries = Vec::new();
    let mut remaining = contents.as_str();
    while let Some(pos) = remaining.find("\"name\"") {
        remaining = &remaining[pos + 6..];
        // skip to value
        if let Some(q1) = remaining.find('"') {
            remaining = &remaining[q1 + 1..];
            if let Some(q2) = remaining.find('"') {
                let name = remaining[..q2].to_string();
                remaining = &remaining[q2 + 1..];
                // find median_ns
                if let Some(mp) = remaining.find("\"median_ns\"") {
                    let after = &remaining[mp + 11..];
                    // skip to colon and number
                    if let Some(cp) = after.find(':') {
                        let num_start = &after[cp + 1..];
                        let num_str: String = num_start
                            .chars()
                            .skip_while(|c| c.is_whitespace())
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        if let Ok(ns) = num_str.parse::<u128>() {
                            entries.push((name, ns));
                        }
                    }
                }
            }
        }
    }
    entries
}

// ── Output: table ────────────────────────────────────────────────────────────

fn format_duration(ns: u128) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.3} s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        format!("{:.3} ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.3} µs", ns as f64 / 1_000.0)
    } else {
        format!("{} ns", ns)
    }
}

fn format_throughput(ops: f64) -> String {
    if ops >= 1_000_000.0 {
        format!("{:.2}M ops/s", ops / 1_000_000.0)
    } else if ops >= 1_000.0 {
        format!("{:.2}K ops/s", ops / 1_000.0)
    } else {
        format!("{:.2} ops/s", ops)
    }
}

fn print_table(results: &[BenchResult], baseline: &Option<Vec<(String, u128)>>) {
    let baseline_map: Vec<(&str, u128)> = baseline
        .as_ref()
        .map(|b| b.iter().map(|(n, v)| (n.as_str(), *v)).collect())
        .unwrap_or_default();

    let find_baseline = |name: &str| -> Option<u128> {
        baseline_map.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    };

    let name_w = results.iter().map(|r| r.name.len()).max().unwrap_or(20).max(20);
    let time_w = 14;
    let tp_w = 16;
    let status_w = 6;
    let delta_w = 20;

    let has_baseline = baseline.is_some();
    let row_w = if has_baseline {
        name_w + time_w + tp_w + status_w + delta_w + 14
    } else {
        name_w + time_w + tp_w + status_w + 11
    };

    println!();
    println!("╔{:═<row_w$}╗", "");
    println!(
        "║ {:^w$} ║",
        "GRIFT BENCHMARK RESULTS",
        w = row_w - 2
    );
    println!("╠{:═<row_w$}╣", "");

    if has_baseline {
        println!(
            "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$}  {:>dw$} ║",
            "Benchmark",
            "Time",
            "Throughput",
            "Status",
            "vs Baseline",
            nw = name_w,
            tw = time_w,
            tpw = tp_w,
            sw = status_w,
            dw = delta_w,
        );
    } else {
        println!(
            "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$} ║",
            "Benchmark",
            "Time",
            "Throughput",
            "Status",
            nw = name_w,
            tw = time_w,
            tpw = tp_w,
            sw = status_w,
        );
    }
    println!("╠{:─<row_w$}╣", "");

    let mut current_cat = "";
    let mut total_ns: u128 = 0;
    let mut pass = 0usize;
    let mut fail = 0usize;

    for r in results {
        if r.category != current_cat {
            current_cat = &r.category;
            println!(
                "║ {:<w$} ║",
                format!("── {} ──", current_cat),
                w = row_w - 2
            );
        }

        total_ns += r.median_ns;
        if r.ok {
            pass += 1;
        } else {
            fail += 1;
        }

        let status = if r.ok { " OK " } else { "FAIL" };
        let time_str = format_duration(r.median_ns);
        let tp_str = format_throughput(r.throughput_ops_per_sec);

        if has_baseline {
            let delta_str = if let Some(base_ns) = find_baseline(&r.name) {
                if base_ns == 0 {
                    "N/A".to_string()
                } else {
                    let pct = ((r.median_ns as f64 - base_ns as f64) / base_ns as f64) * 100.0;
                    if pct > REGRESSION_THRESHOLD_PCT {
                        // Regression - red
                        format!("\x1b[31m+{:.1}% (slower)\x1b[0m", pct)
                    } else if pct < -REGRESSION_THRESHOLD_PCT {
                        // Improvement - green
                        format!("\x1b[32m{:.1}% (faster)\x1b[0m", pct)
                    } else {
                        format!("{:+.1}%", pct)
                    }
                }
            } else {
                "new".to_string()
            };
            println!(
                "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$}  {:>dw$} ║",
                r.name,
                time_str,
                tp_str,
                status,
                delta_str,
                nw = name_w,
                tw = time_w,
                tpw = tp_w,
                sw = status_w,
                dw = delta_w,
            );
        } else {
            println!(
                "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$} ║",
                r.name,
                time_str,
                tp_str,
                status,
                nw = name_w,
                tw = time_w,
                tpw = tp_w,
                sw = status_w,
            );
        }
    }

    println!("╠{:─<row_w$}╣", "");
    let summary = format!("{} pass, {} fail", pass, fail);
    let total_str = format_duration(total_ns);
    if has_baseline {
        println!(
            "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$}  {:>dw$} ║",
            "TOTAL",
            total_str,
            summary,
            "",
            "",
            nw = name_w,
            tw = time_w,
            tpw = tp_w,
            sw = status_w,
            dw = delta_w,
        );
    } else {
        println!(
            "║ {:<nw$}  {:>tw$}  {:>tpw$}  {:>sw$} ║",
            "TOTAL",
            total_str,
            summary,
            "",
            nw = name_w,
            tw = time_w,
            tpw = tp_w,
            sw = status_w,
        );
    }
    println!("╚{:═<row_w$}╝", "");
}

// ── Benchmark definitions ────────────────────────────────────────────────────

fn all_benchmarks() -> Vec<BenchDesc> {
    vec![
        // ── eval: Eval Loop & Dispatch Overhead ──────────────────────────
        BenchDesc {
            name: "eval/bare-literal",
            category: "eval",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) 42)))",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "eval/symbol-resolve-1",
            category: "eval",
            code: "(let ((x 42)) x)",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "eval/symbol-resolve-10",
            category: "eval",
            code: "(let ((a 1)) (let ((b 2)) (let ((c 3)) (let ((d 4)) (let ((e 5)) (let ((f 6)) (let ((g 7)) (let ((h 8)) (let ((i 9)) (let ((j 10)) j))))))))))",
            expected: Some(Value::Number(10)),
        },
        BenchDesc {
            name: "eval/operative-dispatch",
            category: "eval",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (begin 42))))",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "eval/applicative-dispatch",
            category: "eval",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (+ 1 2))))",
            expected: Some(Value::Number(3)),
        },
        BenchDesc {
            name: "eval/compound-call",
            category: "eval",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) ((lambda (x) x) 42))))",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "eval/tco-trampoline",
            category: "eval",
            code: "(let loop ((n 100000)) (if (= n 0) 0 (loop (- n 1))))",
            expected: Some(Value::Number(0)),
        },
        BenchDesc {
            name: "eval/non-tail-fib25",
            category: "eval",
            code: r#"
            (begin
              (define! fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
              (fib 25))
            "#,
            expected: Some(Value::Number(75025)),
        },

        // ── arena: Arena & Allocation ────────────────────────────────────
        BenchDesc {
            name: "arena/cons-build-10k",
            category: "arena",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (car (build 10000 (list))))
            "#,
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "arena/cons-build-100k",
            category: "arena",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (car (build 100000 (list))))
            "#,
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "arena/number-alloc",
            category: "arena",
            code: "(let loop ((n 100000)) (if (= n 0) 0 (loop (- n 1))))",
            expected: Some(Value::Number(0)),
        },
        BenchDesc {
            name: "arena/transient-churn",
            category: "arena",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 1000 (list)))
              (define! mymap (lambda (f l) (if (null? l) (list) (cons (f (car l)) (mymap f (cdr l))))))
              (car (mymap (lambda (x) (+ x 1)) lst)))
            "#,
            expected: Some(Value::Number(2)),
        },

        // ── gc: GC Pressure & Root Management ────────────────────────────
        BenchDesc {
            name: "gc/root-stack-light",
            category: "gc",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (+ 1))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "gc/root-stack-medium",
            category: "gc",
            code: "(let loop ((n 1000) (v 0)) (if (= n 0) v (loop (- n 1) (+ 1 2 3 4 5 6 7 8 9 10))))",
            expected: Some(Value::Number(55)),
        },
        BenchDesc {
            name: "gc/forced-collect",
            category: "gc",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (build 10000 (list))
              (gc-collect)
              42)
            "#,
            expected: Some(Value::Number(42)),
        },

        // ── operative: Operative Hot Paths ───────────────────────────────
        BenchDesc {
            name: "operative/quote",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (quote 42))))",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "operative/if-true",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (if #t 1 2))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "operative/if-false",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (if #f 1 2))))",
            expected: Some(Value::Number(2)),
        },
        BenchDesc {
            name: "operative/cond-1clause",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (cond (#t 42)))))",
            expected: Some(Value::Number(42)),
        },
        BenchDesc {
            name: "operative/cond-5clause",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (cond (#f 1) (#f 2) (#f 3) (#f 4) (#t 5)))))",
            expected: Some(Value::Number(5)),
        },
        BenchDesc {
            name: "operative/let-1bind",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (let ((a 1)) a))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "operative/let-5bind",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (let ((a 1)(b 2)(c 3)(d 4)(e 5)) e))))",
            expected: Some(Value::Number(5)),
        },
        BenchDesc {
            name: "operative/let-named",
            category: "operative",
            code: "(let loop ((n 100000)) (if (= n 0) 0 (loop (- n 1))))",
            expected: Some(Value::Number(0)),
        },
        BenchDesc {
            name: "operative/lambda-create",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (begin (lambda (x) x) 1))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "operative/begin-10",
            category: "operative",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (begin 1 2 3 4 5 6 7 8 9 10))))",
            expected: Some(Value::Number(10)),
        },
        BenchDesc {
            name: "operative/and-2arg",
            category: "operative",
            code: "(let loop ((n 10000) (v #f)) (if (= n 0) v (loop (- n 1) (and #t #t))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "operative/or-2arg",
            category: "operative",
            code: "(let loop ((n 10000) (v #f)) (if (= n 0) v (loop (- n 1) (or #f #t))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "operative/define-seq",
            category: "operative",
            code: r#"
            (begin
              (define! a0 0) (define! a1 1) (define! a2 2) (define! a3 3) (define! a4 4)
              (define! a5 5) (define! a6 6) (define! a7 7) (define! a8 8) (define! a9 9)
              (define! b0 10) (define! b1 11) (define! b2 12) (define! b3 13) (define! b4 14)
              (define! b5 15) (define! b6 16) (define! b7 17) (define! b8 18) (define! b9 19)
              (define! c0 20) (define! c1 21) (define! c2 22) (define! c3 23) (define! c4 24)
              (define! c5 25) (define! c6 26) (define! c7 27) (define! c8 28) (define! c9 29)
              (define! d0 30) (define! d1 31) (define! d2 32) (define! d3 33) (define! d4 34)
              (define! d5 35) (define! d6 36) (define! d7 37) (define! d8 38) (define! d9 39)
              (define! e0 40) (define! e1 41) (define! e2 42) (define! e3 43) (define! e4 44)
              (define! e5 45) (define! e6 46) (define! e7 47) (define! e8 48) (define! e9 49)
              (define! f0 50) (define! f1 51) (define! f2 52) (define! f3 53) (define! f4 54)
              (define! f5 55) (define! f6 56) (define! f7 57) (define! f8 58) (define! f9 59)
              (define! g0 60) (define! g1 61) (define! g2 62) (define! g3 63) (define! g4 64)
              (define! g5 65) (define! g6 66) (define! g7 67) (define! g8 68) (define! g9 69)
              (define! h0 70) (define! h1 71) (define! h2 72) (define! h3 73) (define! h4 74)
              (define! h5 75) (define! h6 76) (define! h7 77) (define! h8 78) (define! h9 79)
              (define! i0 80) (define! i1 81) (define! i2 82) (define! i3 83) (define! i4 84)
              (define! i5 85) (define! i6 86) (define! i7 87) (define! i8 88) (define! i9 89)
              (define! j0 90) (define! j1 91) (define! j2 92) (define! j3 93) (define! j4 94)
              (define! j5 95) (define! j6 96) (define! j7 97) (define! j8 98) (define! j9 99)
              j9)
            "#,
            expected: Some(Value::Number(99)),
        },
        BenchDesc {
            name: "operative/set-mutate",
            category: "operative",
            code: r#"
            (begin
              (define! x 0)
              (define! env (current-environment))
              (let loop ((n 1000))
                (if (= n 0) x
                  (begin (set! env x n) (loop (- n 1))))))
            "#,
            expected: Some(Value::Number(1)),
        },

        // ── builtin: Applicative Builtins ────────────────────────────────
        BenchDesc {
            name: "builtin/add-2arg",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (+ 1 2))))",
            expected: Some(Value::Number(3)),
        },
        BenchDesc {
            name: "builtin/add-20arg",
            category: "builtin",
            code: "(let loop ((n 1000) (v 0)) (if (= n 0) v (loop (- n 1) (+ 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20))))",
            expected: Some(Value::Number(210)),
        },
        BenchDesc {
            name: "builtin/sub-unary",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (- 5))))",
            expected: Some(Value::Number(-5)),
        },
        BenchDesc {
            name: "builtin/div",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (/ 100 3))))",
            expected: Some(Value::Number(33)),
        },
        BenchDesc {
            name: "builtin/compare",
            category: "builtin",
            code: "(let loop ((n 10000) (v #f)) (if (= n 0) v (loop (- n 1) (< 1 2))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "builtin/cons-cell",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (car (cons 1 2)))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "builtin/car-cdr",
            category: "builtin",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 1000 (list)))
              (define! traverse (lambda (l) (if (null? l) 0 (+ (car l) (traverse (cdr l))))))
              (traverse lst))
            "#,
            expected: Some(Value::Number(500500)),
        },
        BenchDesc {
            name: "builtin/list-10",
            category: "builtin",
            code: "(let loop ((n 1000) (v 0)) (if (= n 0) v (loop (- n 1) (car (list 1 2 3 4 5 6 7 8 9 10)))))",
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "builtin/type-pred",
            category: "builtin",
            code: "(let loop ((n 10000) (v #f)) (if (= n 0) v (loop (- n 1) (number? 42))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "builtin/eq-same",
            category: "builtin",
            code: "(let loop ((n 10000) (v #f)) (if (= n 0) v (loop (- n 1) (eq? 1 1))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "builtin/equal-deep",
            category: "builtin",
            code: "(let loop ((n 1000) (v #f)) (if (= n 0) v (loop (- n 1) (equal? (list 1 2 (list 3 4)) (list 1 2 (list 3 4))))))",
            expected: Some(Value::Boolean(true)),
        },
        BenchDesc {
            name: "builtin/eval-reentrant",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (eval (quote (+ 1 2))))))",
            expected: Some(Value::Number(3)),
        },
        BenchDesc {
            name: "builtin/apply-fn",
            category: "builtin",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (apply + (list 1 2 3)))))",
            expected: Some(Value::Number(6)),
        },
        BenchDesc {
            name: "builtin/wrap-unwrap",
            category: "builtin",
            code: r#"
            (begin
              (define! my-op (unwrap +))
              (let loop ((n 1000) (v 0))
                (if (= n 0) v (loop (- n 1) (begin (wrap my-op) 1)))))
            "#,
            expected: Some(Value::Number(1)),
        },

        // ── stdlib: Standard Library ─────────────────────────────────────
        BenchDesc {
            name: "stdlib/map-100",
            category: "stdlib",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 100 (list)))
              (car (map (lambda (x) (+ x 1)) lst)))
            "#,
            expected: Some(Value::Number(2)),
        },
        BenchDesc {
            name: "stdlib/filter-100",
            category: "stdlib",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 100 (list)))
              (car (filter (lambda (x) (> x 50)) lst)))
            "#,
            expected: Some(Value::Number(51)),
        },
        BenchDesc {
            name: "stdlib/length-100",
            category: "stdlib",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 100 (list)))
              (let loop ((n 100) (v 0)) (if (= n 0) v (loop (- n 1) (length lst)))))
            "#,
            expected: Some(Value::Number(100)),
        },
        BenchDesc {
            name: "stdlib/append-100",
            category: "stdlib",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst1 (build 100 (list)))
              (define! lst2 (build 100 (list)))
              (let loop ((n 100) (v 0)) (if (= n 0) v (loop (- n 1) (length (append lst1 lst2))))))
            "#,
            expected: Some(Value::Number(200)),
        },
        BenchDesc {
            name: "stdlib/map-repeated-1000",
            category: "stdlib",
            code: r#"
            (begin
              (define! lst (list 1 2 3 4 5))
              (let loop ((n 1000) (v 0))
                (if (= n 0) v (loop (- n 1) (car (map (lambda (x) (+ x 1)) lst))))))
            "#,
            expected: Some(Value::Number(2)),
        },

        // ── composite: Real-World Composite Workloads ────────────────────
        BenchDesc {
            name: "composite/fib-naive-25",
            category: "composite",
            code: r#"
            (begin
              (define! fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
              (fib 25))
            "#,
            expected: Some(Value::Number(75025)),
        },
        BenchDesc {
            name: "composite/fib-naive-30",
            category: "composite",
            code: r#"
            (begin
              (define! fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
              (fib 30))
            "#,
            expected: Some(Value::Number(832040)),
        },
        BenchDesc {
            name: "composite/fib-tco-1m",
            category: "composite",
            code: r#"
            (begin
              (define! loop-1m (lambda (n acc)
                (if (= n 0) acc (loop-1m (- n 1) (+ acc 1)))))
              (loop-1m 1000000 0))
            "#,
            expected: Some(Value::Number(1000000)),
        },
        BenchDesc {
            name: "composite/ackermann-3-7",
            category: "composite",
            code: r#"
            (begin
              (define! ack (lambda (m n)
                (cond
                  ((= m 0) (+ n 1))
                  ((= n 0) (ack (- m 1) 1))
                  (#t (ack (- m 1) (ack m (- n 1)))))))
              (ack 3 7))
            "#,
            expected: Some(Value::Number(1021)),
        },
        BenchDesc {
            name: "composite/mergesort-500",
            category: "composite",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! my-length (lambda (l) (let loop ((x l) (n 0)) (if (null? x) n (loop (cdr x) (+ n 1))))))
              (define! take (lambda (lst n)
                (if (= n 0) (list)
                  (cons (car lst) (take (cdr lst) (- n 1))))))
              (define! drop (lambda (lst n)
                (if (= n 0) lst (drop (cdr lst) (- n 1)))))
              (define! merge (lambda (a b)
                (cond
                  ((null? a) b)
                  ((null? b) a)
                  ((<= (car a) (car b)) (cons (car a) (merge (cdr a) b)))
                  (#t (cons (car b) (merge a (cdr b)))))))
              (define! mergesort (lambda (lst)
                (let ((len (my-length lst)))
                  (if (<= len 1) lst
                    (let ((mid (/ len 2)))
                      (merge (mergesort (take lst mid))
                             (mergesort (drop lst mid))))))))
              (define! data (build 500 (list)))
              (car (mergesort data)))
            "#,
            expected: Some(Value::Number(1)),
        },
        BenchDesc {
            name: "composite/env-heavy-50",
            category: "composite",
            code: r#"
            (let ((a 1))
            (let ((b (+ a 1)))
            (let ((c (+ b 1)))
            (let ((d (+ c 1)))
            (let ((e (+ d 1)))
            (let ((f (+ e 1)))
            (let ((g (+ f 1)))
            (let ((h (+ g 1)))
            (let ((i (+ h 1)))
            (let ((j (+ i 1)))
            (let ((k (+ j 1)))
            (let ((l (+ k 1)))
            (let ((m (+ l 1)))
            (let ((n (+ m 1)))
            (let ((o (+ n 1)))
            (let ((p (+ o 1)))
            (let ((q (+ p 1)))
            (let ((r (+ q 1)))
            (let ((s (+ r 1)))
            (let ((t (+ s 1)))
            (let ((u (+ t 1)))
            (let ((v (+ u 1)))
            (let ((w (+ v 1)))
            (let ((x (+ w 1)))
            (let ((y (+ x 1)))
            (let ((z (+ y 1)))
            (let ((a2 (+ z 1)))
            (let ((b2 (+ a2 1)))
            (let ((c2 (+ b2 1)))
            (let ((d2 (+ c2 1)))
            (let ((e2 (+ d2 1)))
            (let ((f2 (+ e2 1)))
            (let ((g2 (+ f2 1)))
            (let ((h2 (+ g2 1)))
            (let ((i2 (+ h2 1)))
            (let ((j2 (+ i2 1)))
            (let ((k2 (+ j2 1)))
            (let ((l2 (+ k2 1)))
            (let ((m2 (+ l2 1)))
            (let ((n2 (+ m2 1)))
            (let ((o2 (+ n2 1)))
            (let ((p2 (+ o2 1)))
            (let ((q2 (+ p2 1)))
            (let ((r2 (+ q2 1)))
            (let ((s2 (+ r2 1)))
            (let ((t2 (+ s2 1)))
            (let ((u2 (+ t2 1)))
            (let ((v2 (+ u2 1)))
            (let ((w2 (+ v2 1)))
            (let ((x2 (+ w2 1)))
              (+ a b c d e f g h i j k l m n o p q r s t u v w x y z
                 a2 b2 c2 d2 e2 f2 g2 h2 i2 j2 k2 l2 m2 n2 o2 p2 q2 r2 s2 t2 u2 v2 w2 x2)
            ))))))))))))))))))))))))))))))))))))))))))))))))))"#,
            expected: Some(Value::Number(1275)),
        },
        BenchDesc {
            name: "composite/tak-18-12-6",
            category: "composite",
            code: r#"
            (begin
              (define! tak (lambda (x y z)
                (if (>= y x) z
                  (tak (tak (- x 1) y z)
                       (tak (- y 1) z x)
                       (tak (- z 1) x y)))))
              (tak 18 12 6))
            "#,
            expected: Some(Value::Number(7)),
        },

        // ── parse: Parsing & Reading ─────────────────────────────────────
        BenchDesc {
            name: "parse/simple-expr",
            category: "parse",
            code: "(let loop ((n 10000) (v 0)) (if (= n 0) v (loop (- n 1) (eval (quote (+ 1 2))))))",
            expected: Some(Value::Number(3)),
        },
        BenchDesc {
            name: "parse/stdlib-cold",
            category: "parse",
            code: r#"
            (begin
              (define! build (lambda (n acc) (if (= n 0) acc (build (- n 1) (cons n acc)))))
              (define! lst (build 10 (list)))
              (length lst))
            "#,
            expected: Some(Value::Number(10)),
        },
        BenchDesc {
            name: "parse/stdlib-hot",
            category: "parse",
            code: r#"
            (begin
              (define! lst (list 1 2 3 4 5))
              (let loop ((n 1000) (v 0)) (if (= n 0) v (loop (- n 1) (length lst)))))
            "#,
            expected: Some(Value::Number(5)),
        },
    ]
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let builder = std::thread::Builder::new()
        .name("bench".into())
        .stack_size(STACK_SIZE);
    let handler = builder
        .spawn(run_benchmarks)
        .expect("failed to spawn thread");
    handler.join().expect("benchmark thread panicked");
}

fn run_benchmarks() {
    let cfg = parse_args();
    let descs = all_benchmarks();

    let filtered: Vec<&BenchDesc> = descs
        .iter()
        .filter(|d| {
            cfg.filter
                .as_ref()
                .map_or(true, |f| d.name.contains(f.as_str()))
        })
        .collect();

    eprintln!(
        "Running {} benchmarks (iterations={}, warmup={})...",
        filtered.len(),
        cfg.iterations,
        cfg.warmup,
    );

    let mut results = Vec::new();
    for (i, desc) in filtered.iter().enumerate() {
        eprint!(
            "  [{}/{}] {}...",
            i + 1,
            filtered.len(),
            desc.name
        );
        let r = run_single(desc, cfg.warmup, cfg.iterations);
        eprintln!(
            " {} ({})",
            format_duration(r.median_ns),
            if r.ok { "ok" } else { "FAIL" }
        );
        results.push(r);
    }

    let baseline = cfg.baseline.as_ref().map(|p| parse_baseline(p));

    if cfg.json {
        print!("{}", results_to_json(&results));
    } else {
        print_table(&results, &baseline);
    }

    if let Some(ref path) = cfg.save {
        let json = results_to_json(&results);
        let mut f = std::fs::File::create(path).expect("failed to create save file");
        f.write_all(json.as_bytes())
            .expect("failed to write save file");
        eprintln!("Results saved to {}", path);
    }
}
