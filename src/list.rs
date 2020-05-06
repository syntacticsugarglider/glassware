use crate::React;

#[derive(Debug)]
pub enum Event<T: React> {
    Update { index: u64, event: T::Event },
    Remove(u64),
}

pub trait List<T: React>: React<Event = Event<T>> {}
