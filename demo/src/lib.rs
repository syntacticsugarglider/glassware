use core::{
    borrow::BorrowMut,
    fmt::Display,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Stream};
use glassware::{list, ChannelList, ChannelRegister, List, Register};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;
use web_sys::{window, Element, Node};

type ChannelData<T> = ChannelList<ChannelRegister<T>>;

pub struct ListView<T: Display, U: Register<T>, S: List<U>> {
    marker: PhantomData<(T, U)>,
    events: S::Stream,
    data: Vec<Element>,
    node: Node,
}

impl<T: Display, U: Register<T>, S: List<U>> ListView<T, U, S> {
    fn new<R: BorrowMut<S>>(mut list: R, node: Node) -> Self {
        ListView {
            events: list.borrow_mut().react(),
            marker: PhantomData,
            data: vec![],
            node,
        }
    }
}

impl<T: Display, U: Register<T>, S: List<U>> Unpin for ListView<T, U, S> {}

impl<T: Display, U: Register<T>, S: List<U>> Future for ListView<T, U, S>
where
    S::Stream: Unpin,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            if let Some(event) = ready!(Pin::new(&mut self.events).poll_next(cx)) {
                use list::Event::{Remove, Update};
                match event {
                    Update { index, event } => {
                        let index = index as usize;
                        let formatted = format!("{}", event);
                        let item = if let Some(data) = self.data.get_mut(index) {
                            data
                        } else {
                            let node = window()
                                .unwrap()
                                .document()
                                .unwrap()
                                .create_element("div")
                                .unwrap();

                            self.node
                                .insert_before(
                                    node.as_ref(),
                                    self.data.get(index as usize + 1).map(AsRef::as_ref),
                                )
                                .unwrap();

                            self.data.push(node);

                            self.data.last_mut().unwrap()
                        };
                        item.set_text_content(Some(&formatted));
                    }
                    Remove(index) => {
                        let index = index as usize;

                        if let Some(item) = self.data.get_mut(index) {
                            item.remove()
                        }

                        self.data.remove(index);
                    }
                }
            } else {
                return Poll::Ready(());
            }
        }
    }
}

#[wasm_bindgen]
pub fn entry() {
    spawn_local(async move {
        let data = vec![10, 32, 54, 63];

        let data: ChannelData<u32> = data.into();

        let document = window().unwrap().document().unwrap();

        let element = document.create_element("div").unwrap();

        document
            .body()
            .unwrap()
            .append_child(element.as_ref())
            .unwrap();

        ListView::new(data, element.into()).await;
    });
}
