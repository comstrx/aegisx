use std::net::IpAddr;

use ipnet::IpNet;

use super::arch::Nets;

impl Nets {

    pub fn new ( nets: &[IpNet] ) -> Self {

        let mut v4: Vec<( u32, u32 )> = Vec::new();
        let mut v6: Vec<( u128, u128 )> = Vec::new();

        for net in nets {

            match net {
                IpNet::V4(net) => v4.push(( u32::from(net.network()), u32::from(net.broadcast()) )),
                IpNet::V6(net) => v6.push(( u128::from(net.network()), u128::from(net.broadcast()) )),
            }

        }

        Self { v4: Self::merge(v4), v6: Self::merge(v6) }

    }

    pub fn contains ( &self, ip: IpAddr ) -> bool {

        match ip {
            IpAddr::V4(ip) => Self::within(&self.v4, u32::from(ip)),
            IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
                Some(mapped) => Self::within(&self.v4, u32::from(mapped)),
                None => Self::within(&self.v6, u128::from(ip)),
            },
        }

    }

    pub fn is_empty ( &self ) -> bool {

        self.v4.is_empty() && self.v6.is_empty()

    }

    fn merge <T: Ord + Copy> ( mut ranges: Vec<( T, T )> ) -> Box<[( T, T )]> {

        ranges.sort_unstable();

        let mut merged: Vec<( T, T )> = Vec::with_capacity(ranges.len());

        for ( start, end ) in ranges {

            match merged.last_mut() {
                Some(last) if start <= last.1 => { if end > last.1 { last.1 = end; } }
                _ => merged.push(( start, end )),
            }

        }

        merged.into_boxed_slice()

    }

    fn within <T: Ord + Copy> ( ranges: &[( T, T )], value: T ) -> bool {

        let index = ranges.partition_point(|( start, _ )| *start <= value);

        index.checked_sub(1).and_then(|last| ranges.get(last)).is_some_and(|( _, end )| *end >= value)

    }

}
