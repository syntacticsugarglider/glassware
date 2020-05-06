use crate::React;

pub trait Register<T>: React<Event = T> {}
