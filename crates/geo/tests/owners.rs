//! Builds small owners files from made-up data and checks lookups, and that
//! damaged files are refused without panicking.

use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::GzEncoder;
use portlight_geo::{NetworkOwner, OwnerDb, build};

const SAMPLE: &str = "\
1.0.0.0\t1.0.0.255\t13335\tUS\tCLOUDFLARENET - Cloudflare, Inc.
1.0.1.0\t1.0.3.255\t0\tNone\tNot routed
1.0.4.0\t1.0.7.255\t38803\tAU\tGTELECOM-AS-AP - Gtelecom Pty Ltd
1.0.8.0\t1.0.8.255\t38803\tAU\tGTELECOM-AS-AP - Gtelecom Pty Ltd
1.0.9.0\t1.0.9.255\t38803\tAU\tGTELECOM-AS-AP - Gtelecom Pty Ltd
8.8.8.0\t8.8.8.255\t15169\tUS\tGOOGLE - Google LLC
9.9.9.0\t9.9.9.255\t64500\tNone\tMYSTERY-NET Mystery Networks Ltd
10.0.0.0\t10.255.255.255\t0\tNone\tNot routed
20.0.0.0\t20.0.0.255\t64501\tUnknown\tLEVEL3
20.0.1.0\t20.0.1.255\t13335\tUS\tCLOUDFLARENET - Cloudflare, Inc.
255.255.255.0\t255.255.255.255\t64502\tde\tTOP-AS Top Of The World GmbH
this line is not a range
2001:4860::\t2001:4860:ffff:ffff:ffff:ffff:ffff:ffff\t15169\tUS\tGOOGLE - Google LLC
2606:4700::\t2606:4700:ffff:ffff:ffff:ffff:ffff:ffff\t13335\tUS\tCLOUDFLARENET - Cloudflare, Inc.
2606:4701::\t2606:4701:ffff:ffff:ffff:ffff:ffff:ffff\t13335\tUS\tCLOUDFLARENET - Cloudflare, Inc.
2a00::\t2a00:ffff:ffff:ffff:ffff:ffff:ffff:ffff\t0\tNone\tNot routed
";

fn built(tsv: &[u8]) -> (Vec<u8>, portlight_geo::BuildStats) {
    let mut out = Vec::new();
    let stats = build(tsv, &mut out, 1_700_000_000).expect("build");
    (out, stats)
}

fn sample_db() -> OwnerDb {
    OwnerDb::from_bytes(built(SAMPLE.as_bytes()).0).expect("open")
}

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn owner(asn: u32, name: &str, country: Option<&str>) -> Option<NetworkOwner> {
    Some(NetworkOwner { asn, name: name.to_owned(), country: country.map(str::to_owned) })
}

#[test]
fn stats_count_joined_ranges_and_unique_owners() {
    let (bytes, stats) = built(SAMPLE.as_bytes());
    // The three Gtelecom ranges become one, and the two Cloudflare IPv6 ones too.
    assert_eq!(stats.v4_ranges, 7);
    assert_eq!(stats.v6_ranges, 2);
    // Cloudflare, Gtelecom, Google, Mystery, LEVEL3, Top: each stored once.
    assert_eq!(stats.owners, 6);
    assert_eq!(stats.bytes, bytes.len());
    let db = OwnerDb::from_bytes(bytes).unwrap();
    assert_eq!(db.len(), (7, 2));
    assert!(!db.is_empty());
    assert_eq!(db.built_at(), 1_700_000_000);
}

#[test]
fn lookups_hit_range_edges_exactly() {
    let db = sample_db();
    let cloudflare = owner(13335, "Cloudflare, Inc.", Some("US"));
    assert_eq!(db.lookup(ip("1.0.0.0")), cloudflare);
    assert_eq!(db.lookup(ip("1.0.0.128")), cloudflare);
    assert_eq!(db.lookup(ip("1.0.0.255")), cloudflare);
    assert_eq!(db.lookup(ip("0.255.255.255")), None);
    assert_eq!(db.lookup(ip("8.8.8.8")), owner(15169, "Google LLC", Some("US")));
    assert_eq!(db.lookup(ip("8.8.7.255")), None);
    assert_eq!(db.lookup(ip("8.8.9.0")), None);
    assert_eq!(db.lookup(ip("0.0.0.0")), None);
}

#[test]
fn unrouted_ranges_and_gaps_have_no_owner() {
    let db = sample_db();
    assert_eq!(db.lookup(ip("1.0.1.0")), None);
    assert_eq!(db.lookup(ip("1.0.3.255")), None);
    assert_eq!(db.lookup(ip("10.1.2.3")), None);
    assert_eq!(db.lookup(ip("192.168.1.1")), None);
    assert_eq!(db.lookup(ip("2a00::1")), None);
    assert_eq!(db.lookup(ip("::1")), None);
    assert_eq!(db.lookup(ip("ffff::1")), None);
}

#[test]
fn joined_ranges_cover_every_address_between() {
    let db = sample_db();
    let gtelecom = owner(38803, "Gtelecom Pty Ltd", Some("AU"));
    for address in ["1.0.4.0", "1.0.7.255", "1.0.8.0", "1.0.8.255", "1.0.9.0", "1.0.9.255"] {
        assert_eq!(db.lookup(ip(address)), gtelecom, "{address}");
    }
    assert_eq!(db.lookup(ip("1.0.10.0")), None);
    let cloudflare = owner(13335, "Cloudflare, Inc.", Some("US"));
    assert_eq!(db.lookup(ip("2606:4700:ffff:ffff:ffff:ffff:ffff:ffff")), cloudflare);
    assert_eq!(db.lookup(ip("2606:4701::")), cloudflare);
    assert_eq!(db.lookup(ip("2606:4702::")), None);
}

#[test]
fn missing_countries_and_the_top_address() {
    let db = sample_db();
    assert_eq!(db.lookup(ip("9.9.9.9")), owner(64500, "Mystery Networks Ltd", None));
    assert_eq!(db.lookup(ip("20.0.0.1")), owner(64501, "LEVEL3", None));
    // A lower-case country is stored in capitals.
    assert_eq!(db.lookup(ip("255.255.255.255")), owner(64502, "Top Of The World GmbH", Some("DE")));
}

#[test]
fn ipv6_and_ipv4_written_as_ipv6() {
    let db = sample_db();
    assert_eq!(db.lookup(ip("2001:4860:4860::8888")), owner(15169, "Google LLC", Some("US")));
    assert_eq!(db.lookup(ip("2001:4860::")), owner(15169, "Google LLC", Some("US")));
    assert_eq!(db.lookup(ip("2001:485f:ffff:ffff:ffff:ffff:ffff:ffff")), None);
    assert_eq!(db.lookup(ip("::ffff:8.8.8.8")), owner(15169, "Google LLC", Some("US")));
    assert_eq!(db.lookup(ip("::ffff:10.0.0.1")), None);
}

#[test]
fn gzipped_input_builds_the_same_file() {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(SAMPLE.as_bytes()).unwrap();
    let gz = encoder.finish().unwrap();
    assert_eq!(built(&gz).0, built(SAMPLE.as_bytes()).0);
}

#[test]
fn input_without_ranges_is_refused() {
    assert!(build("hello\nworld\n".as_bytes(), Vec::new(), 0).is_err());
    assert!(build(&b""[..], Vec::new(), 0).is_err());
    // A gzip download that was cut short.
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(SAMPLE.as_bytes()).unwrap();
    let gz = encoder.finish().unwrap();
    assert!(build(&gz[..gz.len() / 2], Vec::new(), 0).is_err());
}

#[test]
fn files_that_are_not_owners_files_are_refused() {
    assert!(OwnerDb::from_bytes(Vec::new()).is_err());
    assert!(OwnerDb::from_bytes(b"PLOWNERS".to_vec()).is_err());
    assert!(OwnerDb::from_bytes(vec![0; 4096]).is_err());
    let mut wrong_version = built(SAMPLE.as_bytes()).0;
    wrong_version[8] = 2;
    assert!(OwnerDb::from_bytes(wrong_version).is_err());
}

/// A tiny xorshift generator, so the damage is the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Addresses worth trying on a damaged file: the sample's edges and some random ones.
fn probe(db: &OwnerDb, rng: &mut Rng) {
    for text in ["1.0.0.0", "1.0.7.255", "8.8.8.8", "255.255.255.255", "2606:4700::1", "::ffff:1.0.0.1"] {
        let _ = db.lookup(ip(text));
    }
    for _ in 0..16 {
        let _ = db.lookup(IpAddr::from((rng.next() as u32).to_be_bytes()));
        let _ = db.lookup(IpAddr::from((u128::from(rng.next()) << 64 | u128::from(rng.next())).to_be_bytes()));
    }
    let _ = db.len();
}

#[test]
fn every_truncation_is_refused() {
    let bytes = built(SAMPLE.as_bytes()).0;
    for len in 0..bytes.len() {
        assert!(OwnerDb::from_bytes(bytes[..len].to_vec()).is_err(), "cut at {len}");
    }
    assert!(OwnerDb::from_bytes(bytes).is_ok());
}

#[test]
fn random_damage_never_panics() {
    let bytes = built(SAMPLE.as_bytes()).0;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut opened = 0;
    for round in 0..20_000 {
        let mut damaged = bytes.clone();
        let flips = 1 + round % 4;
        for _ in 0..flips {
            let at = (rng.next() % damaged.len() as u64) as usize;
            // Mostly the header and tables, where damage matters most.
            let at = if round % 2 == 0 { at % 128 } else { at };
            damaged[at] ^= 1 << (rng.next() % 8);
        }
        if let Ok(db) = OwnerDb::from_bytes(damaged) {
            opened += 1;
            probe(&db, &mut rng);
        }
    }
    // Damage inside names or record values can still be a sound file.
    assert!(opened > 0);
}

#[test]
fn damaged_header_numbers_are_refused() {
    let bytes = built(SAMPLE.as_bytes()).0;
    let mut rng = Rng(42);
    for field in (16..64).step_by(4) {
        for value in [0u32, 1, 63, 64, u32::MAX, u32::MAX / 12, bytes.len() as u32, rng.next() as u32] {
            let mut damaged = bytes.clone();
            damaged[field..field + 4].copy_from_slice(&value.to_le_bytes());
            if let Ok(db) = OwnerDb::from_bytes(damaged) {
                probe(&db, &mut rng);
            }
        }
    }
}

/// Runs against the real download when it is there:
/// `cargo test -p portlight-geo -- --ignored` with PORTLIGHT_IP2ASN set to
/// the path of ip2asn-combined.tsv.gz.
#[test]
#[ignore]
fn real_data() {
    let Some(path) = std::env::var_os("PORTLIGHT_IP2ASN").map(PathBuf::from) else {
        eprintln!("PORTLIGHT_IP2ASN is not set; skipping");
        return;
    };
    let input = std::fs::File::open(&path).expect("open the download");
    let mut out = Vec::new();
    let stats = build(input, &mut out, 0).expect("build");
    assert!(stats.v4_ranges > 100_000 && stats.v6_ranges > 10_000, "{stats:?}");
    assert!(stats.bytes < 20_000_000, "{stats:?}");
    let db = OwnerDb::from_bytes(out).unwrap();
    let asn = |text: &str| db.lookup(ip(text)).map(|owner| owner.asn);
    assert_eq!(asn("1.1.1.1"), Some(13335));
    assert_eq!(asn("8.8.8.8"), Some(15169));
    assert_eq!(asn("2606:4700:4700::1111"), Some(13335));
    assert_eq!(asn("192.168.1.1"), None);
    assert_eq!(db.lookup(ip("1.1.1.1")).unwrap().name, "Cloudflare, Inc.");
}
