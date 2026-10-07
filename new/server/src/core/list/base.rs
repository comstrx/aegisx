use super::arch::Few;

impl <T, const N: usize> Few <T, N> {

    pub fn new () -> Self {

        Self { items: std::array::from_fn(|_| None), len: 0 }

    }

    pub fn push ( &mut self, item: T ) -> bool {

        let Some(slot) = self.items.get_mut(self.len) else { return false; };

        *slot = Some(item);
        self.len += 1;

        true

    }

    pub fn iter ( &self ) -> impl Iterator<Item = &T> {

        self.items.iter().take(self.len).flatten()

    }

    pub fn len ( &self ) -> usize {

        self.len

    }

    pub fn is_empty ( &self ) -> bool {

        self.len == 0

    }

}

impl <T, const N: usize> Default for Few <T, N> {

    fn default () -> Self {

        Self::new()

    }

}
