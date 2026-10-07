use std::net::IpAddr;

use aegisx::core::net::Nets;
use ipnet::IpNet;

fn nets ( list: &[&str] ) -> Nets {

    Nets::new(&list.iter().map(|text| text.parse::<IpNet>().expect("network")).collect::<Vec<_>>())

}

fn ip ( text: &str ) -> IpAddr {

    text.parse().expect("address")

}

#[test]
fn membership_follows_the_listed_networks () {

    let set = nets(&["10.0.0.0/8", "192.168.1.0/24", "203.0.113.7/32", "2001:db8::/32"]);

    assert!(set.contains(ip("10.255.255.255")));
    assert!(set.contains(ip("192.168.1.200")));
    assert!(set.contains(ip("203.0.113.7")));
    assert!(set.contains(ip("2001:db8:ffff::1")));
    assert!(!set.contains(ip("9.255.255.255")));
    assert!(!set.contains(ip("11.0.0.0")));
    assert!(!set.contains(ip("192.168.2.1")));
    assert!(!set.contains(ip("203.0.113.8")));
    assert!(!set.contains(ip("2001:db9::1")));

}

#[test]
fn overlapping_networks_merge_and_mapped_addresses_match_their_ipv4_form () {

    let set = nets(&["10.0.0.0/8", "10.1.0.0/16", "10.0.0.0/7", "0.0.0.0/32"]);

    assert!(set.contains(ip("11.2.3.4")));
    assert!(set.contains(ip("0.0.0.0")));
    assert!(set.contains(ip("::ffff:10.9.8.7")));
    assert!(!set.contains(ip("12.0.0.1")));
    assert!(!set.contains(ip("0.0.0.1")));
    assert!(Nets::default().is_empty());
    assert!(!Nets::default().contains(ip("127.0.0.1")));

}
