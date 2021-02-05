use futures::{
    future::{ready, Ready},
    stream::iter,
    task::Spawn,
    Future, FutureExt, StreamExt, TryFuture, TryFutureExt,
};
use looking_glass::{match_any, typed, Erase, ProtocolAny};
use protocol::{allocated::ProtocolError, protocol};
use protocol_mve_transport::ProtocolMveTransport;
use std::{
    cell::RefCell,
    collections::HashMap,
    convert::Infallible,
    fmt::{Debug, Display},
    panic,
    pin::Pin,
    rc::Rc,
};
use thiserror::Error;
use wasm_bindgen::{prelude::wasm_bindgen, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlElement;

#[derive(Clone)]
pub struct Spawner;

impl Spawn for Spawner {
    fn spawn_obj(
        &self,
        future: futures::future::FutureObj<'static, ()>,
    ) -> Result<(), futures::task::SpawnError> {
        spawn_local(future);
        Ok(())
    }
}

#[derive(Error, Clone)]
#[error("{display}")]
pub struct TransportError {
    display: String,
    debug: String,
}

impl Debug for TransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(formatter, "{}", self.debug)
    }
}

impl TransportError {
    pub fn new<T: Display + Debug>(item: T) -> Self {
        Self {
            display: format!("{}", item),
            debug: format!("{:?}", item),
        }
    }
}

impl From<ProtocolError> for TransportError {
    fn from(item: ProtocolError) -> Self {
        TransportError::new(item)
    }
}

#[typed]
#[protocol]
#[derive(Hash, PartialEq, Eq)]
pub struct Element(pub u64);

#[typed]
#[protocol]
pub trait Ui<T> {
    type Add: TryFuture<Ok = Element>;

    fn add_erased(&mut self, component: ProtocolAny<T>) -> Self::Add;
    fn box_clone(&self) -> Box<dyn Ui<T, Add = Self::Add>>;
}

impl<U, T: TryFuture<Ok = Element>> Clone for Box<dyn Ui<U, Add = T>> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub trait UiConstructor<T>
where
    <Self::Handle as TryFuture>::Ok: Ui<T>,
{
    type Handle: TryFuture;
    type Construct: TryFuture<Ok = ()>;

    fn handle(&self) -> Self::Handle;
    fn construct(self: Box<Self>, root: Element) -> Self::Construct;
}

pub trait UiExt<T>: Ui<T> {
    fn add<U: Erase<T>>(&mut self, item: U) -> Self::Add {
        self.add_erased(item.erase())
    }
}

impl<T: Ui<U>, U> UiExt<U> for T {}

#[typed]
#[protocol]
pub struct Button(
    String,
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), ProtocolError>> + Send>> + Send>,
);

#[typed]
#[protocol]
#[derive(Debug)]
pub struct Heading(String);

#[typed]
#[protocol]
#[derive(Debug)]
pub struct Paragraph(String);

impl Heading {
    pub fn new<T: Into<String>>(item: T) -> Self {
        Heading(item.into())
    }
}

#[typed]
#[protocol]
pub struct StaticStack(Vec<Element>, Direction);

#[typed]
#[protocol]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Direction {
    Horizontal,
    Vertical,
}

impl StaticStack {
    pub async fn new<T, U: Ui<T>>(
        ui: &mut U,
        elements: Vec<ProtocolAny<T>>,
        direction: Direction,
    ) -> Self {
        StaticStack(
            iter(elements.into_iter().map(move |item| {
                ui.add_erased(item)
                    .unwrap_or_else(|_| panic!())
                    .into_stream()
            }))
            .flatten()
            .collect()
            .await,
            direction,
        )
    }
    pub fn new_from_elements(elements: Vec<Element>, direction: Direction) -> Self {
        StaticStack(elements, direction)
    }
}

#[derive(Clone)]
pub struct DomUi<T: Spawn + Clone> {
    document: web_sys::Document,
    spawner: T,
    registry: Rc<RefCell<(HashMap<Element, HtmlElement>, u64)>>,
}

impl<T: Spawn + Clone + Send + Unpin + 'static> Ui<ProtocolMveTransport> for DomUi<T> {
    type Add = Pin<Box<dyn Future<Output = Result<Element, TransportError>>>>;

    fn add_erased(&mut self, component: ProtocolAny<ProtocolMveTransport>) -> Self::Add {
        let document = self.document.clone();
        let reg = self.registry.clone();
        let spawner = self.spawner.clone();

        Box::pin(async move {
            let mut registry = reg.borrow_mut();
            let index = registry.1;
            registry.1 += 1;

            let new_el = match_any!(spawner, component,
                Heading(data) => {
                    let element: HtmlElement = document.create_element("h1").unwrap().dyn_into().unwrap();

                    element.set_text_content(Some(&data));

                    element
                }
                Paragraph(data) => {
                    let element: HtmlElement = document.create_element("p").unwrap().dyn_into().unwrap();

                    element.set_text_content(Some(&data));

                    element
                }
                StaticStack(elements, direction) => {
                    let container: HtmlElement = document.create_element("div").unwrap().dyn_into().unwrap();

                    if let Direction::Horizontal = direction {
                        container.set_attribute("style", "display: flex; flex-direction: row;").unwrap();
                    }

                    for element in elements {
                        let el = registry.0.get(&element).unwrap().clone();
                        container.append_child(&el.into()).unwrap();
                    }

                    container
                }
            )
            .await
            .unwrap();

            reg.borrow_mut().0.insert(Element(index), new_el);

            Ok(Element(index))
        })
    }
    fn box_clone(&self) -> Box<dyn Ui<ProtocolMveTransport, Add = Self::Add>> {
        Box::new(self.clone())
    }
}

pub struct DomRoot<T: Spawn + Clone> {
    root: HtmlElement,
    spawner: T,
    document: web_sys::Document,
    registry: Rc<RefCell<(HashMap<Element, HtmlElement>, u64)>>,
}

impl<T: Spawn + Clone + Send + Unpin + 'static> UiConstructor<ProtocolMveTransport> for DomRoot<T> {
    type Handle = Ready<Result<DomUi<T>, Infallible>>;
    type Construct = Ready<Result<(), Infallible>>;

    fn construct(self: Box<Self>, root: Element) -> Self::Construct {
        self.root
            .append_child(&self.registry.borrow_mut().0.remove(&root).unwrap().into())
            .unwrap();
        ready(Ok(()))
    }
    fn handle(&self) -> Self::Handle {
        ready(Ok(DomUi {
            spawner: self.spawner.clone(),
            registry: self.registry.clone(),
            document: self.document.clone(),
        }))
    }
}

impl<T: Spawn + Clone> DomRoot<T> {
    pub fn new(spawner: T) -> Box<Self> {
        let document = web_sys::window().unwrap().document().unwrap();
        Box::new(Self {
            root: document.body().unwrap(),
            document,
            spawner,
            registry: Rc::new(RefCell::new((HashMap::new(), 0u64))),
        })
    }
}

#[wasm_bindgen(start)]
pub fn entry() {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    spawn_local(async {
        let ui = DomRoot::new(Spawner);
        let mut handle = ui.handle().await.unwrap();
        let element = StaticStack::new(
            &mut handle,
            vec![
                Heading("hello there".into()).erase(),
                Paragraph("this is some body text".into()).erase(),
            ],
            Direction::Vertical,
        )
        .await;
        let element = handle.add(element).await.unwrap();
        ui.construct(element).await.unwrap();
    });
}
