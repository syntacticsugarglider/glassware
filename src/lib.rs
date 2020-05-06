use core::{
    iter,
    pin::Pin,
    task::{Context, Poll},
};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    future, stream, Stream, StreamExt,
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

pub struct ListMap<T: React> {
    stream: T::Stream,
    index: u64,
}

impl<T: React> ListMap<T> {
    fn new(index: u64, stream: T::Stream) -> Self {
        ListMap { stream, index }
    }
}

impl<T: React> Stream for ListMap<T>
where
    T::Stream: Unpin,
{
    type Item = list::Event<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx).map(|event| {
            event.map(|event| list::Event::Update {
                event,
                index: self.index,
            })
        })
    }
}

impl<T: Clone + React> React for ChannelList<T>
where
    T::Stream: Unpin,
{
    type Event = list::Event<T>;

    type Stream = stream::SelectAll<
        future::Either<ListMap<T>, stream::Map<UnboundedReceiver<u64>, fn(u64) -> list::Event<T>>>,
    >;

    fn react(&mut self) -> Self::Stream {
        let (sender, receiver) = unbounded();

        self.senders.push(sender);

        stream::select_all(
            self.data
                .iter_mut()
                .enumerate()
                .map(|(index, data)| ListMap::new(index as u64, data.react()).left_stream())
                .chain(iter::once(
                    receiver
                        .map(list::Event::Remove as fn(u64) -> list::Event<T>)
                        .right_stream(),
                )),
        )
    }
}

impl<T: Clone + React> List<T> for ChannelList<T> where T::Stream: Unpin {}
