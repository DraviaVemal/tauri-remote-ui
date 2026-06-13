//! Network helpers for the Remote UI server.
//!
//! Provides:
//! * enumeration of locally reachable IP addresses so the blocking screen can
//!   show every URL a remote browser may connect with;
//! * a subnet membership check used by the `Subnet` origin scope to filter
//!   incoming peers.
//!
//! # License
//! AGPL-3.0-only License
//! Copyright (c) 2025 DraviaVemal
//! See LICENSE file in the root directory.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use if_addrs::{IfAddr, Interface};

use crate::OriginType;

/// A locally-bound interface address along with the netmask required to
/// compute subnet membership.
#[derive(Debug, Clone)]
pub(crate) struct LocalIp {
    pub name: String,
    pub ip: IpAddr,
    pub netmask: IpAddr,
}

/// Enumerate the host's interface addresses. Errors are logged and treated as
/// an empty list so the server can still come up.
pub(crate) fn local_addresses() -> Vec<LocalIp> {
    match if_addrs::get_if_addrs() {
        Ok(ifs) => ifs.into_iter().map(to_local_ip).collect(),
        Err(err) => {
            log::warn!("Failed to enumerate network interfaces: {err}");
            Vec::new()
        }
    }
}

fn to_local_ip(iface: Interface) -> LocalIp {
    let name = iface.name.clone();
    match iface.addr {
        IfAddr::V4(v4) => LocalIp {
            name,
            ip: IpAddr::V4(v4.ip),
            netmask: IpAddr::V4(v4.netmask),
        },
        IfAddr::V6(v6) => LocalIp {
            name,
            ip: IpAddr::V6(v6.ip),
            netmask: IpAddr::V6(v6.netmask),
        },
    }
}

/// Returns only the local interface entries that describe a *valid, bounded*
/// subnet. Entries are filtered out if they would match every peer (or every
/// peer in a family) and therefore offer no real restriction:
///
/// * unspecified interface IP (`0.0.0.0`, `::`)
/// * zero netmask (`0.0.0.0`, `::`) — i.e. a `/0` "default route" entry
/// * IPv6 link-local (`fe80::/10`) — these have non-routable scope and don't
///   correspond to a meaningful subnet for peer filtering
pub(crate) fn trusted_local_subnets() -> Vec<LocalIp> {
    local_addresses()
        .into_iter()
        .filter(is_bounded_subnet)
        .collect()
}

fn is_bounded_subnet(local: &LocalIp) -> bool {
    match (local.ip, local.netmask) {
        (IpAddr::V4(ip), IpAddr::V4(mask)) => {
            !ip.is_unspecified() && u32::from(mask) != 0
        }
        (IpAddr::V6(ip), IpAddr::V6(mask)) => {
            !ip.is_unspecified()
                && u128::from(mask) != 0
                && !is_ipv6_link_local(ip)
        }
        _ => false,
    }
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    // fe80::/10
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Human-readable CIDR-style descriptions of every trusted local subnet, used
/// for diagnostic logging on server start.
pub(crate) fn trusted_subnet_descriptions() -> Vec<String> {
    trusted_local_subnets()
        .iter()
        .map(|local| match (local.ip, local.netmask) {
            (IpAddr::V4(_), IpAddr::V4(mask)) => format!(
                "{} on {} (mask {mask}, /{})",
                local.ip,
                local.name,
                u32::from(mask).count_ones()
            ),
            (IpAddr::V6(_), IpAddr::V6(mask)) => format!(
                "{} on {} (/{})",
                local.ip,
                local.name,
                u128::from(mask).count_ones()
            ),
            _ => format!("{} on {}", local.ip, local.name),
        })
        .collect()
}

/// Returns the list of addresses (without port) a remote browser can use to
/// reach this server given the configured origin scope. The list always
/// includes loopback so the operator can copy/paste from the host machine.
pub(crate) fn reachable_addresses(origin: OriginType) -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    let push_unique = |out: &mut Vec<IpAddr>, ip: IpAddr| {
        if !out.contains(&ip) {
            out.push(ip);
        }
    };
    match origin {
        OriginType::Localhost => {
            push_unique(&mut out, IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
        OriginType::Subnet | OriginType::Any => {
            for entry in local_addresses() {
                // Only surface IPv4 globally; IPv6 link-local addresses are
                // rarely useful in a browser address bar.
                if let IpAddr::V4(v4) = entry.ip {
                    if v4.is_unspecified() {
                        continue;
                    }
                    push_unique(&mut out, IpAddr::V4(v4));
                }
            }
            // Make sure loopback is always available from the host itself.
            push_unique(&mut out, IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
    }
    out
}

/// Returns true if `peer` is allowed by the given origin scope, based on the
/// host's currently visible interface addresses. Logs the decision at
/// `debug`/`warn` level so an operator can audit why a connection was accepted
/// or rejected.
pub(crate) fn peer_allowed(origin: OriginType, peer: IpAddr) -> bool {
    let decision = match origin {
        OriginType::Any => true,
        OriginType::Localhost => is_loopback_ip(peer),
        OriginType::Subnet => {
            if is_loopback_ip(peer) {
                true
            } else if peer.is_unspecified() {
                false
            } else {
                trusted_local_subnets()
                    .iter()
                    .any(|local| same_subnet(local, peer))
            }
        }
    };
    if decision {
        log::debug!("Remote UI: allow peer {peer} (scope: {origin:?})");
    } else {
        log::debug!("Remote UI: deny peer {peer} (scope: {origin:?})");
    }
    decision
}

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || ipv6_to_ipv4_loopback(v6),
    }
}

/// Treat IPv4-mapped IPv6 loopback (`::ffff:127.0.0.1`) as loopback.
fn ipv6_to_ipv4_loopback(v6: Ipv6Addr) -> bool {
    matches!(v6.to_ipv4_mapped(), Some(v4) if v4.is_loopback())
}

fn same_subnet(local: &LocalIp, peer: IpAddr) -> bool {
    match (local.ip, local.netmask, peer) {
        (IpAddr::V4(local_ip), IpAddr::V4(mask), IpAddr::V4(peer)) => {
            mask_v4(local_ip, mask) == mask_v4(peer, mask)
        }
        (IpAddr::V4(local_ip), IpAddr::V4(mask), IpAddr::V6(peer)) => {
            // Allow IPv4-mapped IPv6 peers (`::ffff:a.b.c.d`).
            match peer.to_ipv4_mapped() {
                Some(peer_v4) => mask_v4(local_ip, mask) == mask_v4(peer_v4, mask),
                None => false,
            }
        }
        (IpAddr::V6(local_ip), IpAddr::V6(mask), IpAddr::V6(peer)) => {
            mask_v6(local_ip, mask) == mask_v6(peer, mask)
        }
        _ => false,
    }
}

fn mask_v4(ip: Ipv4Addr, mask: Ipv4Addr) -> u32 {
    u32::from(ip) & u32::from(mask)
}

fn mask_v6(ip: Ipv6Addr, mask: Ipv6Addr) -> u128 {
    u128::from(ip) & u128::from(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(local_ip: &str, mask: &str) -> LocalIp {
        LocalIp {
            name: "test".into(),
            ip: IpAddr::V4(local_ip.parse::<Ipv4Addr>().unwrap()),
            netmask: IpAddr::V4(mask.parse::<Ipv4Addr>().unwrap()),
        }
    }

    fn ip4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }

    fn ip6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn same_subnet_v4_within_24() {
        let local = v4("192.168.1.10", "255.255.255.0");
        assert!(same_subnet(&local, ip4("192.168.1.250")));
        assert!(!same_subnet(&local, ip4("192.168.2.1")));
        assert!(!same_subnet(&local, ip4("10.0.0.1")));
    }

    #[test]
    fn zero_netmask_is_not_a_bounded_subnet() {
        // A /0 entry would otherwise match every IPv4 peer. Filter it out.
        let local = v4("10.0.0.1", "0.0.0.0");
        assert!(!is_bounded_subnet(&local));
    }

    #[test]
    fn unspecified_local_ip_is_not_a_bounded_subnet() {
        let local = v4("0.0.0.0", "255.255.255.0");
        assert!(!is_bounded_subnet(&local));
    }

    #[test]
    fn ipv6_link_local_is_not_a_bounded_subnet() {
        let local = LocalIp {
            name: "lo".into(),
            ip: ip6("fe80::1"),
            netmask: ip6("ffff:ffff:ffff:ffff::"),
        };
        assert!(!is_bounded_subnet(&local));
    }

    #[test]
    fn loopback_is_always_allowed_in_subnet_mode() {
        assert!(peer_allowed(OriginType::Subnet, ip4("127.0.0.1")));
        assert!(peer_allowed(OriginType::Subnet, ip6("::1")));
    }

    #[test]
    fn localhost_mode_rejects_external() {
        assert!(!peer_allowed(OriginType::Localhost, ip4("192.168.1.10")));
        assert!(peer_allowed(OriginType::Localhost, ip4("127.0.0.1")));
    }

    #[test]
    fn ipv4_mapped_ipv6_peer_matches_v4_subnet() {
        let local = v4("192.168.1.10", "255.255.255.0");
        assert!(same_subnet(&local, ip6("::ffff:192.168.1.99")));
        assert!(!same_subnet(&local, ip6("::ffff:10.0.0.1")));
    }
}
