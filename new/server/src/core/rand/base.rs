use crate::core::error::{AppFail, AppResult};
use super::arch::Rng;

impl Rng {

    pub fn seeded () -> AppResult<Self> {

        let mut seed = [0u8; 32];

        getrandom::fill(&mut seed).or_fail("cannot read system entropy")?;

        let mut state = [0u64; 4];

        for ( index, chunk ) in seed.as_chunks::<8>().0.iter().enumerate() {

            state[index] = u64::from_le_bytes(*chunk);

        }

        if state.iter().all(|word| *word == 0) { state[0] = 0x9E37_79B9_7F4A_7C15; }

        Ok(Self { state })

    }

    pub fn next_u64 ( &mut self ) -> u64 {

        let [a, b, c, d] = self.state;
        let result = (b.wrapping_mul(5)).rotate_left(7).wrapping_mul(9);
        let t = b << 17;

        self.state[2] = c ^ a;
        self.state[3] = d ^ b;
        self.state[1] = b ^ self.state[2];
        self.state[0] = a ^ self.state[3];
        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);

        result

    }

    pub fn next_u128 ( &mut self ) -> u128 {

        (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64())

    }

    pub fn below ( &mut self, bound: usize ) -> usize {

        if bound == 0 { return 0; }

        (self.next_u64() % bound as u64) as usize

    }

    pub fn uuid ( &mut self, out: &mut [u8; 36] ) {

        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut bytes = self.next_u128().to_le_bytes();

        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;

        let mut cursor = 0;

        for ( index, byte ) in bytes.iter().enumerate() {

            if matches!(index, 4 | 6 | 8 | 10) { out[cursor] = b'-'; cursor += 1; }

            out[cursor] = HEX[(byte >> 4) as usize];
            out[cursor + 1] = HEX[(byte & 0x0f) as usize];
            cursor += 2;

        }

    }

}
