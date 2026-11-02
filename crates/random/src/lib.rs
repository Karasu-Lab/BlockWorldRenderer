pub trait RandomSource {
    fn next_u64(&mut self) -> u64;

    fn fork(&mut self) -> Box<dyn RandomSource>;

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u64);

    impl RandomSource for Counter {
        fn next_u64(&mut self) -> u64 {
            self.0 += 1;
            self.0 << 32
        }

        fn fork(&mut self) -> Box<dyn RandomSource> {
            Box::new(Counter(self.0))
        }
    }

    #[test]
    fn defaults_derive_from_next_u64() {
        let mut random = Counter(0);
        assert_eq!(random.next_u32(), 1);
        let f = random.next_f64();
        assert!((0.0..1.0).contains(&f));
    }

    #[test]
    fn fork_is_usable_through_the_trait_object() {
        let mut random = Counter(0);
        let mut forked = random.fork();
        assert_eq!(forked.next_u32(), 1);
    }
}
