use bwr_entity::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Tick(pub u64);

pub trait Tickable {
    fn tick(&mut self, tick: Tick);
}

pub trait TickScheduler {
    fn current(&self) -> Tick;

    fn schedule(&mut self, delay: u64, task: Box<dyn FnOnce(Tick) + Send>);

    fn advance(&mut self) -> Tick;
}

pub trait EntityTicker {
    fn tick_entities(&mut self, entities: &mut [Box<dyn Entity>], tick: Tick);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u64);

    impl Tickable for Counter {
        fn tick(&mut self, tick: Tick) {
            self.0 = tick.0;
        }
    }

    #[test]
    fn tickable_receives_the_tick() {
        let mut counter = Counter(0);
        counter.tick(Tick(9));
        assert_eq!(counter.0, 9);
    }
}
