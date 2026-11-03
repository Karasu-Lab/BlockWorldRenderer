use random::RandomSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WorldSeed(pub i64);

impl From<i64> for WorldSeed {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<WorldSeed> for i64 {
    fn from(seed: WorldSeed) -> Self {
        seed.0
    }
}

pub trait SeedSource {
    fn seed(&self) -> WorldSeed;

    fn derive(&self, salt: u64) -> WorldSeed;

    fn random_for(&self, salt: u64) -> Box<dyn RandomSource>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_and_from_i64() {
        let seed = WorldSeed::from(42);
        assert_eq!(i64::from(seed), 42);
    }
}
