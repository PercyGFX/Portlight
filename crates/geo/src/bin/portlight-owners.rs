//! Builds and checks Portlight's owners file.
//!
//! ```text
//! portlight-owners build <ip2asn-combined.tsv[.gz]> <owners.bin>
//! portlight-owners lookup <owners.bin> <ip>...
//! portlight-owners bench <owners.bin>
//! ```

use std::fs::File;
use std::hint::black_box;
use std::io::BufWriter;
use std::net::IpAddr;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use portlight_geo::{OwnerDb, build};

const USAGE: &str = "usage:
  portlight-owners build <ip2asn-combined.tsv[.gz]> <owners.bin>
  portlight-owners lookup <owners.bin> <ip>...
  portlight-owners bench <owners.bin>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["build", input, output] => run_build(input, output),
        ["lookup", file, ips @ ..] if !ips.is_empty() => run_lookup(file, ips),
        ["bench", file] => run_bench(file),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run_build(input: &str, output: &str) -> Result<(), String> {
    let started = Instant::now();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let reader = File::open(input).map_err(|e| format!("cannot open {input}: {e}"))?;
    let writer = File::create(output).map_err(|e| format!("cannot create {output}: {e}"))?;
    let stats = build(reader, BufWriter::new(writer), now).map_err(|e| format!("build failed: {e}"))?;
    println!(
        "{output}: {} IPv4 ranges, {} IPv6 ranges, {} owners, {:.1} MB, in {:.1} s",
        stats.v4_ranges,
        stats.v6_ranges,
        stats.owners,
        stats.bytes as f64 / 1_000_000.0,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn open(file: &str) -> Result<OwnerDb, String> {
    let started = Instant::now();
    let db = OwnerDb::open(Path::new(file)).map_err(|e| format!("cannot open {file}: {e}"))?;
    let (v4, v6) = db.len();
    eprintln!("opened {file} in {:.1} ms: {v4} IPv4 and {v6} IPv6 ranges", started.elapsed().as_secs_f64() * 1000.0);
    Ok(db)
}

fn run_lookup(file: &str, ips: &[&str]) -> Result<(), String> {
    let db = open(file)?;
    for text in ips {
        let ip: IpAddr = text.parse().map_err(|_| format!("{text} is not an IP address"))?;
        match db.lookup(ip) {
            Some(owner) => {
                println!("{ip}\tAS{}\t{}\t{}", owner.asn, owner.country.as_deref().unwrap_or("--"), owner.name)
            }
            None => println!("{ip}\t(no owner: private, reserved or not routed)"),
        }
    }
    Ok(())
}

/// Times a million lookups of spread-out addresses.
fn run_bench(file: &str) -> Result<(), String> {
    let db = open(file)?;
    const COUNT: u32 = 1_000_000;
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let v4: Vec<IpAddr> = (0..COUNT).map(|_| IpAddr::from((next() as u32).to_be_bytes())).collect();
    let v6: Vec<IpAddr> = (0..COUNT)
        .map(|_| {
            // Somewhere in 2000::/4, where the routed IPv6 addresses are.
            let high = 0x2000_0000_0000_0000 | (next() & 0x0fff_ffff_ffff_ffff);
            IpAddr::from((u128::from(high) << 64 | u128::from(next())).to_be_bytes())
        })
        .collect();
    for (label, ips) in [("IPv4", &v4), ("IPv6", &v6)] {
        let started = Instant::now();
        let found = ips.iter().filter(|&&ip| black_box(db.lookup(ip)).is_some()).count();
        let each = started.elapsed().as_secs_f64() * 1e9 / f64::from(COUNT);
        println!("{label}: {each:.0} ns per lookup, {found} of {COUNT} random addresses have an owner");
    }
    Ok(())
}
