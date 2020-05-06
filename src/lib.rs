use core::iter;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    stream,
    stream::BoxStream,
    Stream, StreamExt,
};

pub mod list;
pub mod register;

pub use list::List;
pub use register::Register;

pub trait React {
    type Event;

    type Stream: Stream<Item = Self::Event>;
    fn react(&mut self) -> Self::Stream;
}

#[derive(Clone, Debug)]
pub struct ChannelRegister<T: Clone> {
    data: T,
    senders: Vec<UnboundedSender<T>>,
}

impl<T: Clone> From<T> for ChannelRegister<T> {
    fn from(data: T) -> Self {
        ChannelRegister {
            data,
            senders: vec![],
        }
    }
}

impl<T: Clone> React for ChannelRegister<T> {
    type Event = T;

    type Stream = UnboundedReceiver<T>;
    fn react(&mut self) -> Self::Stream {
        let (sender, receiver) = unbounded();

        sender.unbounded_send(self.data.clone()).unwrap();

        self.senders.push(sender);

        receiver
    }
}

impl<T: Clone> register::Register<T> for ChannelRegister<T> {}

pub struct ChannelList<T: Clone + React> {
    data: Vec<T>,
    senders: Vec<UnboundedSender<u64>>,
}

impl<T: Clone + React + From<U>, U> From<Vec<U>> for ChannelList<T> {
    fn from(input: Vec<U>) -> Self {
        ChannelList {
            data: input.into_iter().map(From::from).collect(),
            senders: vec![],
        }
    }
}

impl<T: Clone + React> React for ChannelList<T>
where
    T::Stream: Unpin + Send + 'static,
{
    type Event = list::Event<T>;

    type Stream = BoxStream<'static, list::Event<T>>;
    fn react(&mut self) -> Self::Stream {
        let (sender, receiver) = unbounded();

        self.senders.push(sender);

        Box::pin(stream::select_all(
            self.data
                .iter_mut()
                .enumerate()
                .map(|(index, data)| {
                    let index = index as u64;
                    data.react()
                        .map(move |event| list::Event::<T>::Update { event, index })
                        .left_stream()
                })
                .chain(iter::once(
                    receiver
                        .map(|index| list::Event::Remove(index))
                        .right_stream(),
                )),
        ))
    }
}

impl<T: Clone + React> List<T> for ChannelList<T> where T::Stream: Unpin + Send + 'static {}
