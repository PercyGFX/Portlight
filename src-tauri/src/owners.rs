//! Who owns a server's network, from the bundled offline list (`owners.bin`,
//! built from the public-domain iptoasn.com data). Nothing is looked up online.

use std::net::IpAddr;
use std::sync::OnceLock;

use portlight_core::OwnerInfo;
use portlight_geo::OwnerDb;

const FILE: &str = "owners.bin";
const HOME_NETWORK: &str = "Your home network";

/// The list, loaded on first use. `None` when the file is missing, e.g. in a
/// development build before `scripts/update-owners.ps1` has been run.
fn db() -> Option<&'static OwnerDb> {
    static DB: OnceLock<Option<OwnerDb>> = OnceLock::new();
    DB.get_or_init(|| {
        let path = std::env::current_exe().ok()?.parent()?.join(FILE);
        match OwnerDb::open(&path) {
            Ok(db) => Some(db),
            Err(error) => {
                eprintln!("portlight: no server owner list ({}): {error}", path.display());
                None
            }
        }
    })
    .as_ref()
}

/// When the bundled list was built (Unix seconds) and how many ranges it
/// holds, or nothing when there is no list.
pub fn list_info() -> Option<(u64, usize)> {
    let db = db()?;
    let (v4, v6) = db.len();
    Some((db.built_at(), v4 + v6))
}

/// Who owns the network `ip` belongs to.
pub fn lookup(ip: IpAddr) -> Option<OwnerInfo> {
    let ip = ip.to_canonical();
    if is_local(ip) {
        return Some(OwnerInfo { name: HOME_NETWORK.into(), asn: None, country: None });
    }
    let owner = db()?.lookup(ip)?;
    Some(OwnerInfo { name: owner.name, asn: Some(owner.asn), country: owner.country })
}

/// Addresses that stay inside the home or office network.
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_network_addresses_need_no_list() {
        for ip in ["192.168.1.1", "10.0.0.5", "172.16.4.4", "169.254.1.1", "fe80::1", "fd00::5"] {
            let owner = lookup(ip.parse().unwrap()).unwrap();
            assert_eq!(owner.name, HOME_NETWORK, "{ip}");
        }
        assert!(!is_local("100.64.0.1".parse().unwrap()));
        assert!(!is_local("8.8.8.8".parse().unwrap()));
    }
}
